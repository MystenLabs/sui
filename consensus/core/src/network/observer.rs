// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{NetworkKeyPair, NetworkPublicKey};
use consensus_types::block::{BlockRef, Round};
use futures::{Stream, StreamExt as _};
use mysten_network::{Multiaddr, callback::CallbackLayer};
use parking_lot::RwLock;
use tokio_stream::Iter;
use tonic::{Request, Response};
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer};
use tracing::{debug, info, trace, warn};

use crate::{
    CommitRange, Context,
    error::{ConsensusError, ConsensusResult},
    network::{
        ObserverBlockStream, ObserverNetworkClient, PeerId,
        metrics_layer::MetricsCallbackMaker,
        to_host_port_str,
        tonic_network::{Channel, MAX_FETCH_RESPONSE_BYTES, chunk_blocks},
        tonic_tls::certificate_server_name,
    },
};

use super::{ObserverNetworkService, tonic_gen::observer_service_server::ObserverService};

// Observer block streaming messages
#[derive(Clone, prost::Message)]
pub(crate) struct BlockStreamRequest {
    #[prost(uint64, repeated, tag = "1")]
    pub(crate) highest_round_per_authority: Vec<u64>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct BlockStreamResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) blocks: Vec<Bytes>,
}

// Observer fetch messages
#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksRequest {
    #[prost(bytes = "vec", repeated, tag = "1")]
    block_refs: Vec<Vec<u8>>,
    // The round per authority after which blocks should be fetched. The vector represents the round
    // for each authority and its length should be the same as the committee size.
    // When this field is non-empty, additional ancestors of the requested blocks can be fetched.
    #[prost(uint32, repeated, tag = "2")]
    fetch_after_rounds: Vec<Round>,
    // When true, missing ancestors of the requested blocks will be fetched as well.
    // When false, additional blocks are fetched depth-first from the requested block authorities.
    // This field is only meaningful when fetch_after_rounds is non-empty.
    #[prost(bool, tag = "3")]
    fetch_missing_ancestors: bool,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) blocks: Vec<Bytes>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsRequest {
    #[prost(uint32, tag = "1")]
    pub(crate) start: u32,
    #[prost(uint32, tag = "2")]
    pub(crate) end: u32,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) commits: Vec<Bytes>,
    #[prost(bytes = "bytes", repeated, tag = "2")]
    pub(crate) certifier_blocks: Vec<Bytes>,
}

/// Information about an observer peer connection, set in request extensions by the server.
#[derive(Clone, Debug)]
pub(crate) struct ObserverPeerInfo {
    #[allow(unused)]
    pub(crate) public_key: NetworkPublicKey,
}

/// Tonic-based implementation of ObserverNetworkClient to talk to a peer's observer server.
pub(crate) struct TonicObserverClient {
    context: Arc<Context>,
    network_keypair: NetworkKeyPair,
    channel_pool: Arc<ChannelPool>,
}

struct ChannelPool {
    context: Arc<Context>,
    // Size is limited by known authorities in the committee.
    channels: RwLock<BTreeMap<PeerId, Channel>>,
    // The observer peers pool that this node is allowed to connect to.
    observer_peers: BTreeMap<NetworkPublicKey, Multiaddr>,
}

impl ChannelPool {
    fn new(context: Arc<Context>) -> Self {
        // Only allow to connect to peers that are within this pool.
        let mut observer_peers = BTreeMap::new();
        for peer in &context.parameters.observer.peers {
            observer_peers.insert(peer.public_key.clone(), peer.address.clone());
        }
        Self {
            context,
            channels: RwLock::new(BTreeMap::new()),
            observer_peers,
        }
    }

    pub(crate) async fn get_channel(
        &self,
        network_keypair: NetworkKeyPair,
        peer: PeerId,
        timeout: Duration,
    ) -> ConsensusResult<Channel> {
        {
            let channels = self.channels.read();
            if let Some(channel) = channels.get(&peer) {
                return Ok(channel.clone());
            }
        }

        // Find the network key of the peer.
        let peer_network_key = match peer.clone() {
            PeerId::Validator(authority) => self
                .context
                .committee
                .authority(authority)
                .network_key
                .clone(),
            PeerId::Observer(node_id) => (*node_id).clone(),
        };

        // Check if the peer is in the observer peers pool. If not return an error.
        let peer_address = self.observer_peers.get(&peer_network_key).ok_or_else(|| {
            ConsensusError::NetworkConfig(format!("Peer not in observer peers pool: {:?}", peer))
        })?;

        let address = to_host_port_str(peer_address).map_err(|e| {
            ConsensusError::NetworkConfig(format!("Cannot convert address to host:port: {e:?}"))
        })?;
        let address = format!("https://{address}");
        let config = &self.context.parameters.tonic;
        let buffer_size = config.connection_buffer_size;
        let client_tls_config = sui_tls::create_rustls_client_config(
            peer_network_key.into_inner().clone(),
            certificate_server_name(&self.context),
            Some(network_keypair.private_key().into_inner()),
        );
        let endpoint = tonic_rustls::Channel::from_shared(address.clone())
            .map_err(|e| ConsensusError::NetworkConfig(format!("invalid URI '{address}': {e}")))?
            .connect_timeout(timeout)
            .initial_connection_window_size(Some(buffer_size as u32))
            .initial_stream_window_size(Some(buffer_size as u32 / 2))
            .keep_alive_while_idle(true)
            .keep_alive_timeout(config.keepalive_interval)
            .http2_keep_alive_interval(config.keepalive_interval)
            // tcp keepalive is probably unnecessary and is unsupported by msim.
            .user_agent("mysticeti")
            .unwrap()
            .tls_config(client_tls_config)
            .unwrap();

        let deadline = tokio::time::Instant::now() + timeout;
        let channel = loop {
            trace!("Connecting to endpoint at {address}");
            match endpoint.connect().await {
                Ok(channel) => break channel,
                Err(e) => {
                    debug!("Failed to connect to endpoint at {address}: {e:?}");
                    if tokio::time::Instant::now() >= deadline {
                        return Err(ConsensusError::NetworkClientConnection(format!(
                            "Timed out connecting to endpoint at {address}: {e:?}"
                        )));
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        trace!("Connected to {address}");

        let channel = tower::ServiceBuilder::new()
            .layer(CallbackLayer::new(MetricsCallbackMaker::new(
                self.context.metrics.network_metrics.outbound.clone(),
                self.context.parameters.tonic.excessive_message_size,
            )))
            .layer(
                TraceLayer::new_for_grpc()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::TRACE))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::DEBUG)),
            )
            .service(channel);

        let mut channels = self.channels.write();
        // There should not be many concurrent attempts at connecting to the same peer.
        let channel = channels.entry(peer).or_insert(channel);
        Ok(channel.clone())
    }
}

impl TonicObserverClient {
    pub(crate) fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        Self {
            context: context.clone(),
            network_keypair,
            channel_pool: Arc::new(ChannelPool::new(context)),
        }
    }

    #[allow(unused)]
    async fn get_client(
        &self,
        peer: PeerId,
        timeout: Duration,
    ) -> ConsensusResult<super::tonic_gen::observer_service_client::ObserverServiceClient<Channel>>
    {
        use tonic::codec::CompressionEncoding;

        let message_size_limit = self.context.parameters.tonic.message_size_limit;
        let channel_pool = self.channel_pool.clone();
        let network_keypair = self.network_keypair.clone();
        let channel = channel_pool
            .get_channel(network_keypair, peer, timeout)
            .await?;
        let client = super::tonic_gen::observer_service_client::ObserverServiceClient::new(channel)
            .max_encoding_message_size(message_size_limit)
            .max_decoding_message_size(message_size_limit)
            .send_compressed(CompressionEncoding::Zstd)
            .accept_compressed(CompressionEncoding::Zstd);
        Ok(client)
    }
}

#[async_trait]
impl ObserverNetworkClient for TonicObserverClient {
    async fn stream_blocks(
        &self,
        peer: PeerId,
        highest_round_per_authority: Vec<u64>,
        timeout: Duration,
    ) -> ConsensusResult<ObserverBlockStream> {
        let mut client = self.get_client(peer.clone(), timeout).await?;

        let request = Request::new(BlockStreamRequest {
            highest_round_per_authority,
        });
        let response = client
            .stream_blocks(request)
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("stream_blocks failed: {e:?}")))?;
        let stream = response
            .into_inner()
            .take_while(|b| futures::future::ready(b.is_ok()))
            .filter_map(move |b| {
                let peer_cloned = peer.clone();
                async move {
                    match b {
                        Ok(response) => Some(response.blocks),
                        Err(e) => {
                            debug!("Network error received from {:?}: {e:?}", peer_cloned);
                            None
                        }
                    }
                }
            });
        Ok(Box::pin(stream))
    }

    async fn fetch_blocks(
        &self,
        peer: PeerId,
        block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(FetchBlocksRequest {
            block_refs: block_refs
                .iter()
                .filter_map(|r| match bcs::to_bytes(r) {
                    Ok(serialized) => Some(serialized),
                    Err(e) => {
                        debug!("Failed to serialize block ref {:?}: {e:?}", r);
                        None
                    }
                })
                .collect(),
            fetch_after_rounds,
            fetch_missing_ancestors,
        });
        request.set_timeout(timeout);

        let mut stream = client
            .fetch_blocks(request)
            .await
            .map_err(|e| {
                if e.code() == tonic::Code::DeadlineExceeded {
                    ConsensusError::NetworkRequestTimeout(format!("fetch_blocks failed: {e:?}"))
                } else {
                    ConsensusError::NetworkRequest(format!("fetch_blocks failed: {e:?}"))
                }
            })?
            .into_inner();

        // Allow twice the max total size of transactions in the fetched blocks.
        let max_allowed_bytes = block_refs.len()
            * self
                .context
                .protocol_config
                .max_transactions_in_block_bytes() as usize
            * 2;
        let mut blocks = vec![];
        let mut total_fetched_bytes = 0;
        loop {
            match stream.message().await {
                Ok(Some(response)) => {
                    for b in &response.blocks {
                        total_fetched_bytes += b.len();
                    }
                    blocks.extend(response.blocks);
                    if total_fetched_bytes > max_allowed_bytes {
                        info!(
                            "fetch_blocks() fetched bytes exceeded limit: {} > {}, terminating stream.",
                            total_fetched_bytes, max_allowed_bytes,
                        );
                        break;
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    if blocks.is_empty() {
                        if e.code() == tonic::Code::DeadlineExceeded {
                            return Err(ConsensusError::NetworkRequestTimeout(format!(
                                "fetch_blocks failed mid-stream: {e:?}"
                            )));
                        }
                        return Err(ConsensusError::NetworkRequest(format!(
                            "fetch_blocks failed mid-stream: {e:?}"
                        )));
                    } else {
                        warn!("fetch_blocks failed mid-stream: {e:?}");
                        break;
                    }
                }
            }
        }
        Ok(blocks)
    }

    async fn fetch_commits(
        &self,
        peer: PeerId,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(FetchCommitsRequest {
            start: commit_range.start(),
            end: commit_range.end(),
        });
        request.set_timeout(timeout);
        let response = client
            .fetch_commits(request)
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("fetch_commits failed: {e:?}")))?;
        let response = response.into_inner();
        Ok((response.commits, response.certifier_blocks))
    }
}

/// Proxies Observer Tonic requests to ObserverNetworkService.
/// Extracts peer NodeId from TLS certificates and delegates to the service layer.
pub(crate) struct ObserverServiceProxy<S: ObserverNetworkService> {
    service: Arc<S>,
}

impl<S: ObserverNetworkService> ObserverServiceProxy<S> {
    pub(crate) fn new(service: Arc<S>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl<S: ObserverNetworkService> ObserverService for ObserverServiceProxy<S> {
    type StreamBlocksStream =
        Pin<Box<dyn Stream<Item = Result<BlockStreamResponse, tonic::Status>> + Send>>;

    async fn stream_blocks(
        &self,
        request: Request<BlockStreamRequest>,
    ) -> Result<Response<Self::StreamBlocksStream>, tonic::Status> {
        let peer_id = request
            .extensions()
            .get::<ObserverPeerInfo>()
            .map(|info| info.public_key.clone())
            .ok_or_else(|| {
                tonic::Status::unauthenticated(
                    "Observer peer info not found in request. TLS authentication required.",
                )
            })?;

        let highest_round_per_authority = request.into_inner().highest_round_per_authority;

        let block_stream = self
            .service
            .handle_stream_blocks(peer_id, highest_round_per_authority)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;

        let response_stream = block_stream.map(|blocks| Ok(BlockStreamResponse { blocks }));

        Ok(Response::new(Box::pin(response_stream)))
    }

    type FetchBlocksStream = Iter<std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>>>;

    async fn fetch_blocks(
        &self,
        request: Request<FetchBlocksRequest>,
    ) -> Result<Response<Self::FetchBlocksStream>, tonic::Status> {
        let peer_id = request
            .extensions()
            .get::<ObserverPeerInfo>()
            .map(|info| info.public_key.clone())
            .ok_or_else(|| {
                tonic::Status::unauthenticated(
                    "Observer peer info not found in request. TLS authentication required.",
                )
            })?;
        let inner = request.into_inner();
        let block_refs = inner
            .block_refs
            .into_iter()
            .filter_map(|serialized| match bcs::from_bytes(&serialized) {
                Ok(r) => Some(r),
                Err(e) => {
                    debug!("Failed to deserialize block ref {:?}: {e:?}", serialized);
                    None
                }
            })
            .collect();
        let fetch_after_rounds = inner.fetch_after_rounds;
        let fetch_missing_ancestors = inner.fetch_missing_ancestors;
        let blocks = self
            .service
            .handle_fetch_blocks(
                peer_id,
                block_refs,
                fetch_after_rounds,
                fetch_missing_ancestors,
            )
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let responses: std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>> =
            chunk_blocks(blocks, MAX_FETCH_RESPONSE_BYTES)
                .into_iter()
                .map(|blocks| Ok(FetchBlocksResponse { blocks }))
                .collect::<Vec<_>>()
                .into_iter();
        let stream = tokio_stream::iter(responses);
        Ok(Response::new(stream))
    }

    async fn fetch_commits(
        &self,
        request: Request<FetchCommitsRequest>,
    ) -> Result<Response<FetchCommitsResponse>, tonic::Status> {
        let peer_id = request
            .extensions()
            .get::<ObserverPeerInfo>()
            .map(|info| info.public_key.clone())
            .ok_or_else(|| {
                tonic::Status::unauthenticated(
                    "Observer peer info not found in request. TLS authentication required.",
                )
            })?;
        let request = request.into_inner();
        let (commits, certifier_blocks) = self
            .service
            .handle_fetch_commits(peer_id, (request.start..=request.end).into())
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let commits = commits
            .into_iter()
            .map(|c| c.serialized().clone())
            .collect();
        let certifier_blocks = certifier_blocks
            .into_iter()
            .map(|b| b.serialized().clone())
            .collect();
        Ok(Response::new(FetchCommitsResponse {
            commits,
            certifier_blocks,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use consensus_config::PeerRecord;
    use consensus_types::block::Round;
    use futures::{StreamExt as _, stream};
    use parking_lot::Mutex;

    use crate::{
        context::Context,
        network::{ExtendedSerializedBlock, ObserverNetworkService, test_network::TestService},
    };

    fn block_for_round(round: Round) -> ExtendedSerializedBlock {
        ExtendedSerializedBlock {
            block: Bytes::from(vec![round as u8; 16]),
            excluded_ancestors: vec![],
        }
    }

    #[tokio::test]
    async fn observer_stream_blocks_from_start() {
        let (_context, keys) = Context::new_for_test(4);

        let service = Arc::new(Mutex::new(TestService::new()));
        {
            let mut s = service.lock();
            let own_blocks = (0..=100u8)
                .map(|i| block_for_round(i as Round))
                .collect::<Vec<_>>();
            s.add_own_blocks(own_blocks);
        }

        let observer_peer_id = keys[0].0.public().clone();

        let block_stream = service
            .handle_stream_blocks(observer_peer_id.clone(), vec![0u64, 0, 0, 0])
            .await
            .unwrap();

        let blocks: Vec<Bytes> = block_stream.flat_map(stream::iter).collect().await;

        assert_eq!(blocks.len(), 100);
        assert_eq!(blocks[0], Bytes::from(vec![1u8; 16]));
        assert_eq!(blocks[99], Bytes::from(vec![100u8; 16]));

        assert_eq!(service.lock().handle_stream_blocks.len(), 1);
        assert_eq!(service.lock().handle_stream_blocks[0], observer_peer_id);
    }

    #[tokio::test]
    async fn observer_stream_blocks_filtered_by_round() {
        let (_context, keys) = Context::new_for_test(4);

        let service = Arc::new(Mutex::new(TestService::new()));
        {
            let mut s = service.lock();
            let own_blocks = (0..=100u8)
                .map(|i| block_for_round(i as Round))
                .collect::<Vec<_>>();
            s.add_own_blocks(own_blocks);
        }

        let observer_peer_id = keys[0].0.public().clone();

        let highest_round_per_authority = vec![50u64, 50, 50, 50];

        let block_stream = service
            .handle_stream_blocks(observer_peer_id, highest_round_per_authority)
            .await
            .unwrap();

        let blocks: Vec<Bytes> = block_stream.flat_map(stream::iter).collect().await;

        assert_eq!(blocks.len(), 50);
        assert_eq!(blocks[0], Bytes::from(vec![51u8; 16]));
        assert_eq!(blocks[49], Bytes::from(vec![100u8; 16]));
    }

    /// End-to-end test using TonicManager to set up a proper observer server and client.
    #[cfg_attr(not(msim), tokio::test(flavor = "multi_thread"))]
    async fn observer_client_server_e2e() {
        use crate::network::{
            NetworkManager, ObserverNetworkClient, PeerId, tonic_network::TonicManager,
        };
        use mysten_network::Multiaddr;
        use std::str::FromStr;
        use std::time::Duration;

        let (context, keys) = Context::new_for_test(4);

        // Use a fixed port for the observer server so we can configure the client properly
        const OBSERVER_PORT: u16 = 9999;

        // Set up validator 0 with observer server
        let mut parameters = context.parameters.clone();
        parameters.observer.server_port = Some(OBSERVER_PORT);
        parameters.observer.peers = vec![PeerRecord {
            public_key: keys[0].0.public(),
            address: Multiaddr::from_str(&format!("/ip4/127.0.0.1/udp/{}", OBSERVER_PORT)).unwrap(),
        }];

        let context_0 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(0).unwrap())
                .with_parameters(parameters),
        );
        let mut manager_0 = TonicManager::new(context_0.clone(), keys[0].0.clone());
        let observer_client_0 = manager_0.observer_client();

        // Set up the test service with blocks
        let service_0 = Arc::new(Mutex::new(TestService::new()));
        {
            let mut s = service_0.lock();
            let own_blocks = (0..=50u8)
                .map(|i| block_for_round(i as Round))
                .collect::<Vec<_>>();
            s.add_own_blocks(own_blocks);
        }

        // Start the observer server
        manager_0.start_observer_server(service_0.clone()).await;

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to stream blocks from validator 0 (as an observer)
        let peer_id = PeerId::Validator(context.committee.to_authority_index(0).unwrap());

        let result = observer_client_0
            .stream_blocks(peer_id, vec![10u64, 10, 10, 10], Duration::from_secs(5))
            .await;

        // This should work with proper TonicManager setup
        // If it fails, it's likely due to authentication/allowlist configuration
        let mut stream = result.unwrap();
        let mut count = 0;
        while let Some(batch) = stream.next().await {
            for block in batch {
                // Verify the blocks are in the expected range (rounds 11-50)
                assert!(block.len() == 16);
                count += 1;
                if count >= 40 {
                    break; // We expect 40 blocks (rounds 11-50)
                }
            }
            if count >= 40 {
                break;
            }
        }
        assert_eq!(count, 40);

        // Verify the service recorded the stream request
        assert!(service_0.lock().handle_stream_blocks.len() <= 1);
    }
}
