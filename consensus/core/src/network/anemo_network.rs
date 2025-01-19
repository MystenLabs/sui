// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    panic,
    sync::Arc,
    time::Duration,
};

use anemo::{
    rpc::Status,
    types::{response::StatusCode, PeerInfo},
    PeerId, Response,
};
use anemo_tower::{
    auth::{AllowedPeers, RequireAuthorizationLayer},
    callback::{CallbackLayer, MakeCallbackHandler, ResponseHandler},
    set_header::{SetRequestHeaderLayer, SetResponseHeaderLayer},
    trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer},
};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, error, warn};

use super::{
    anemo_gen::{
        consensus_rpc_client::ConsensusRpcClient,
        consensus_rpc_server::{ConsensusRpc, ConsensusRpcServer},
    },
    connection_monitor::{AnemoConnectionMonitor, ConnectionMonitorHandle},
    epoch_filter::{AllowedEpoch, EPOCH_HEADER_KEY},
    metrics_layer::{MetricsCallbackMaker, MetricsResponseCallback, SizedRequest, SizedResponse},
    BlockStream, ExtendedSerializedBlock, NetworkClient, NetworkManager, NetworkService,
};
use crate::{
    block::{BlockRef, VerifiedBlock},
    commit::CommitRange,
    context::Context,
    error::{ConsensusError, ConsensusResult},
    CommitIndex, Round,
};

/// Implements Anemo RPC client for Consensus.
pub(crate) struct AnemoClient {
    context: Arc<Context>,
    network: Arc<ArcSwapOption<anemo::Network>>,
}

impl AnemoClient {
    const GET_CLIENT_INTERVAL: Duration = Duration::from_millis(10);

    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            network: Arc::new(ArcSwapOption::default()),
        }
    }

    pub(crate) fn set_network(&self, network: anemo::Network) {
        self.network.store(Some(Arc::new(network)));
    }

    async fn get_client(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<ConsensusRpcClient<anemo::Peer>> {
        let network = loop {
            if let Some(network) = self.network.load_full() {
                break network;
            } else {
                tokio::time::sleep(Self::GET_CLIENT_INTERVAL).await;
            }
        };

        let authority = self.context.committee.authority(peer);
        let peer_id = PeerId(authority.network_key.to_bytes());
        if let Some(peer) = network.peer(peer_id) {
            return Ok(ConsensusRpcClient::new(peer));
        };

        // If we're not connected we'll need to check to see if the Peer is a KnownPeer
        if network.known_peers().get(&peer_id).is_none() {
            return Err(ConsensusError::UnknownNetworkPeer(format!("{}", peer_id)));
        }

        let (mut subscriber, _) = network.subscribe().map_err(|e| {
            ConsensusError::NetworkClientConnection(format!(
                "Cannot subscribe to AnemoNetwork updates: {e:?}"
            ))
        })?;

        let sleep = tokio::time::sleep(timeout);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                recv = subscriber.recv() => match recv {
                    Ok(anemo::types::PeerEvent::NewPeer(pid)) if pid == peer_id => {
                        // We're now connected with the peer, lets try to make a network request
                        if let Some(peer) = network.peer(peer_id) {
                            return Ok(ConsensusRpcClient::new(peer));
                        }
                        warn!("Peer {} should be connected.", peer_id)
                    }
                    Err(RecvError::Closed) => return Err(ConsensusError::Shutdown),
                    Err(RecvError::Lagged(_)) => {
                        subscriber = subscriber.resubscribe();
                        // We lagged behind so we may have missed the connection event
                        if let Some(peer) = network.peer(peer_id) {
                            return Ok(ConsensusRpcClient::new(peer));
                        }
                    }
                    // Just do another iteration
                    _ => {}
                },
                _ = &mut sleep => {
                    return Err(ConsensusError::PeerDisconnected(format!("{}", peer_id)));
                },
            }
        }
    }
}

#[async_trait]
impl NetworkClient for AnemoClient {
    const SUPPORT_STREAMING: bool = false;

    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()> {
        let mut client = self.get_client(peer, timeout).await?;
        let request = SendBlockRequest {
            block: block.serialized().clone(),
        };
        client
            .send_block(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("send_block failed: {e:?}")))?;
        Ok(())
    }

    async fn subscribe_blocks(
        &self,
        _peer: AuthorityIndex,
        _last_received: Round,
        _timeout: Duration,
    ) -> ConsensusResult<BlockStream> {
        unimplemented!("Unimplemented")
    }

    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self.get_client(peer, timeout).await?;
        let request = FetchBlocksRequest {
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
            highest_accepted_rounds,
        };
        let response = client
            .fetch_blocks(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e: Status| {
                if e.status() == StatusCode::RequestTimeout {
                    ConsensusError::NetworkRequestTimeout(format!("fetch_blocks timeout: {e:?}"))
                } else {
                    ConsensusError::NetworkRequest(format!("fetch_blocks failed: {e:?}"))
                }
            })?;
        let body = response.into_body();
        Ok(body.blocks)
    }

    async fn fetch_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        let mut client = self.get_client(peer, timeout).await?;
        let request = FetchCommitsRequest {
            start: commit_range.start(),
            end: commit_range.end(),
        };
        let response = client
            .fetch_commits(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("fetch_blocks failed: {e:?}")))?;
        let response = response.into_body();
        Ok((response.commits, response.certifier_blocks))
    }

    async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self.get_client(peer, timeout).await?;
        let request = FetchLatestBlocksRequest { authorities };
        let response = client
            .fetch_latest_blocks(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e: Status| {
                if e.status() == StatusCode::RequestTimeout {
                    ConsensusError::NetworkRequestTimeout(format!(
                        "fetch_latest_blocks timeout: {e:?}"
                    ))
                } else {
                    ConsensusError::NetworkRequest(format!("fetch_latest_blocks failed: {e:?}"))
                }
            })?;
        let body = response.into_body();
        Ok(body.blocks)
    }

    async fn get_latest_rounds(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
        let mut client = self.get_client(peer, timeout).await?;
        let request = GetLatestRoundsRequest {};
        let response = client
            .get_latest_rounds(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e: Status| {
                if e.status() == StatusCode::RequestTimeout {
                    ConsensusError::NetworkRequestTimeout(format!(
                        "get_latest_rounds timeout: {e:?}"
                    ))
                } else {
                    ConsensusError::NetworkRequest(format!("get_latest_rounds failed: {e:?}"))
                }
            })?;
        let body = response.into_body();
        Ok((body.highest_received, body.highest_accepted))
    }
}

/// Proxies Anemo requests to NetworkService with actual handler implementation.
struct AnemoServiceProxy<S: NetworkService> {
    peer_map: BTreeMap<PeerId, AuthorityIndex>,
    service: Arc<S>,
}

impl<S: NetworkService> AnemoServiceProxy<S> {
    fn new(context: Arc<Context>, service: Arc<S>) -> Self {
        let peer_map = context
            .committee
            .authorities()
            .map(|(index, authority)| {
                let peer_id = PeerId(authority.network_key.to_bytes());
                (peer_id, index)
            })
            .collect();
        Self { peer_map, service }
    }
}

#[async_trait]
impl<S: NetworkService> ConsensusRpc for AnemoServiceProxy<S> {
    async fn send_block(
        &self,
        request: anemo::Request<SendBlockRequest>,
    ) -> Result<anemo::Response<SendBlockResponse>, anemo::rpc::Status> {
        let Some(peer_id) = request.peer_id() else {
            return Err(anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer_id not found",
            ));
        };
        let index = *self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let block = request.into_body().block;
        let block = ExtendedSerializedBlock {
            block,
            excluded_ancestors: vec![],
        };
        self.service
            .handle_send_block(index, block)
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::BadRequest,
                    format!("{e}"),
                )
            })?;
        Ok(Response::new(SendBlockResponse {}))
    }

    async fn fetch_blocks(
        &self,
        request: anemo::Request<FetchBlocksRequest>,
    ) -> Result<anemo::Response<FetchBlocksResponse>, anemo::rpc::Status> {
        let Some(peer_id) = request.peer_id() else {
            return Err(anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer_id not found",
            ));
        };
        let index = *self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let body = request.into_body();
        let block_refs = body
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

        let highest_accepted_rounds = body.highest_accepted_rounds;

        let blocks = self
            .service
            .handle_fetch_blocks(index, block_refs, highest_accepted_rounds)
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::BadRequest,
                    format!("{e}"),
                )
            })?;
        Ok(Response::new(FetchBlocksResponse { blocks }))
    }

    async fn fetch_commits(
        &self,
        request: anemo::Request<FetchCommitsRequest>,
    ) -> Result<anemo::Response<FetchCommitsResponse>, anemo::rpc::Status> {
        let Some(peer_id) = request.peer_id() else {
            return Err(anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer_id not found",
            ));
        };
        let index = *self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let request = request.into_body();
        let (commits, certifier_blocks) = self
            .service
            .handle_fetch_commits(index, (request.start..=request.end).into())
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::InternalServerError,
                    format!("{e}"),
                )
            })?;
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

    async fn fetch_latest_blocks(
        &self,
        request: anemo::Request<FetchLatestBlocksRequest>,
    ) -> Result<anemo::Response<FetchLatestBlocksResponse>, anemo::rpc::Status> {
        let Some(peer_id) = request.peer_id() else {
            return Err(anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer_id not found",
            ));
        };
        let index = *self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let body = request.into_body();
        let blocks = self
            .service
            .handle_fetch_latest_blocks(index, body.authorities)
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::BadRequest,
                    format!("{e}"),
                )
            })?;
        Ok(Response::new(FetchLatestBlocksResponse { blocks }))
    }

    async fn get_latest_rounds(
        &self,
        request: anemo::Request<GetLatestRoundsRequest>,
    ) -> Result<anemo::Response<GetLatestRoundsResponse>, anemo::rpc::Status> {
        let Some(peer_id) = request.peer_id() else {
            return Err(anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer_id not found",
            ));
        };
        let index = *self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let (highest_received, highest_accepted) = self
            .service
            .handle_get_latest_rounds(index)
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::InternalServerError,
                    format!("{e}"),
                )
            })?;
        Ok(Response::new(GetLatestRoundsResponse {
            highest_received,
            highest_accepted,
        }))
    }
}

/// Manages the lifecycle of Anemo network. Typical usage during initialization:
/// 1. Create a new `AnemoManager`.
/// 2. Take `AnemoClient` from `AnemoManager::client()`.
/// 3. Create consensus components.
/// 4. Create `AnemoService` for consensus RPC handler.
/// 5. Install `AnemoService` to `AnemoManager` with `AnemoManager::install_service()`.
pub(crate) struct AnemoManager {
    context: Arc<Context>,
    network_keypair: Option<NetworkKeyPair>,
    client: Arc<AnemoClient>,
    network: Arc<ArcSwapOption<anemo::Network>>,
    connection_monitor_handle: Option<ConnectionMonitorHandle>,
}

impl AnemoManager {
    pub(crate) fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        Self {
            context: context.clone(),
            network_keypair: Some(network_keypair),
            client: Arc::new(AnemoClient::new(context)),
            network: Arc::new(ArcSwapOption::default()),
            connection_monitor_handle: None,
        }
    }
}

impl<S: NetworkService> NetworkManager<S> for AnemoManager {
    type Client = AnemoClient;

    fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        AnemoManager::new(context, network_keypair)
    }

    fn client(&self) -> Arc<Self::Client> {
        self.client.clone()
    }

    async fn install_service(&mut self, service: Arc<S>) {
        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["anemo"])
            .set(1);

        debug!("Starting anemo service");

        let server = ConsensusRpcServer::new(AnemoServiceProxy::new(self.context.clone(), service));
        let authority = self.context.committee.authority(self.context.own_index);
        // By default, bind to the unspecified address to allow the actual address to be assigned.
        // But bind to localhost if it is requested.
        let own_address = if authority.address.is_localhost_ip() {
            authority.address.clone()
        } else {
            authority.address.with_zero_ip()
        };
        let epoch_string: String = self.context.committee.epoch().to_string();
        let inbound_network_metrics = self.context.metrics.network_metrics.inbound.clone();
        let outbound_network_metrics = self.context.metrics.network_metrics.outbound.clone();
        let quinn_connection_metrics = self
            .context
            .metrics
            .network_metrics
            .quinn_connection_metrics
            .clone();
        let all_peer_ids = self
            .context
            .committee
            .authorities()
            .map(|(_i, authority)| PeerId(authority.network_key.to_bytes()));

        let routes = anemo::Router::new()
            .route_layer(RequireAuthorizationLayer::new(AllowedPeers::new(
                all_peer_ids,
            )))
            .route_layer(RequireAuthorizationLayer::new(AllowedEpoch::new(
                epoch_string.clone(),
            )))
            .add_rpc_service(server);

        // TODO: instrument with failpoints.
        let service = tower::ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsCallbackMaker::new(
                inbound_network_metrics,
                self.context.parameters.anemo.excessive_message_size,
            )))
            .layer(SetResponseHeaderLayer::overriding(
                EPOCH_HEADER_KEY.parse().unwrap(),
                epoch_string.clone(),
            ))
            .service(routes);

        let outbound_layer = tower::ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_client_and_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsCallbackMaker::new(
                outbound_network_metrics,
                self.context.parameters.anemo.excessive_message_size,
            )))
            .layer(SetRequestHeaderLayer::overriding(
                EPOCH_HEADER_KEY.parse().unwrap(),
                epoch_string,
            ))
            .into_inner();

        let anemo_config = {
            let mut quic_config = anemo::QuicConfig::default();
            // Allow more concurrent streams for burst activity.
            quic_config.max_concurrent_bidi_streams = Some(10_000);
            // Increase send and receive buffer sizes on the primary, since the primary also
            // needs to fetch payloads.
            // With 200MiB buffer size and ~500ms RTT, the max throughput ~400MiB/s.
            quic_config.stream_receive_window = Some(100 << 20);
            quic_config.receive_window = Some(200 << 20);
            quic_config.send_window = Some(200 << 20);
            quic_config.crypto_buffer_size = Some(1 << 20);
            quic_config.socket_receive_buffer_size = Some(20 << 20);
            quic_config.socket_send_buffer_size = Some(20 << 20);
            quic_config.allow_failed_socket_buffer_size_setting = true;
            quic_config.max_idle_timeout_ms = Some(30_000);
            // Enable keep alives every 5s
            quic_config.keep_alive_interval_ms = Some(5_000);

            let mut config = anemo::Config::default();
            config.quic = Some(quic_config);
            // Set the max_frame_size to be 1 GB to work around the issue of there being too many
            // delegation events in the epoch change txn.
            config.max_frame_size = Some(1 << 30);
            // Set a default timeout of 300s for all RPC requests
            config.inbound_request_timeout_ms = Some(300_000);
            config.outbound_request_timeout_ms = Some(300_000);
            config.shutdown_idle_timeout_ms = Some(1_000);
            config.connectivity_check_interval_ms = Some(2_000);
            config.connection_backoff_ms = Some(1_000);
            config.max_connection_backoff_ms = Some(20_000);
            config
        };

        let mut retries_left = 90;
        let addr = own_address
            .to_anemo_address()
            .unwrap_or_else(|op| panic!("{op}: {own_address}"));
        let private_key_bytes = self.network_keypair.take().unwrap().private_key_bytes();
        let network = loop {
            let network_result = anemo::Network::bind(addr.clone())
                .server_name("consensus")
                .private_key(private_key_bytes)
                .config(anemo_config.clone())
                .outbound_request_layer(outbound_layer.clone())
                .start(service.clone());
            match network_result {
                Ok(n) => {
                    break n;
                }
                Err(e) => {
                    retries_left -= 1;

                    if retries_left <= 0 {
                        panic!("Failed to initialize AnemoNetwork at {addr}! Last error: {e:#?}");
                    }
                    warn!(
                        "Address {addr} should be available for the Consensus service, retrying in one second: {e:#?}",
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };

        let mut known_peer_ids = HashMap::new();
        for (_i, authority) in self.context.committee.authorities() {
            let peer_id = PeerId(authority.network_key.to_bytes());
            let peer_address = match authority.address.to_anemo_address() {
                Ok(addr) => addr,
                // Validations are performed on addresses so this failure should not happen.
                // But it is possible if supported anemo address formats are updated without a
                // feature flag.
                Err(e) => {
                    error!(
                        "Failed to convert {:?} to anemo address: {:?}",
                        authority.address, e
                    );
                    continue;
                }
            };
            let peer_info = PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: vec![peer_address.clone()],
            };
            network.known_peers().insert(peer_info);
            known_peer_ids.insert(peer_id, authority.hostname.clone());
        }

        let connection_monitor_handle = AnemoConnectionMonitor::spawn(
            network.downgrade(),
            quinn_connection_metrics,
            known_peer_ids,
        );

        self.connection_monitor_handle = Some(connection_monitor_handle);
        self.client.set_network(network.clone());
        self.network.store(Some(Arc::new(network)));
    }

    async fn stop(&mut self) {
        if let Some(network) = self.network.load_full() {
            if let Err(e) = network.shutdown().await {
                warn!("Failure when shutting down AnemoNetwork: {e:?}");
            }
            self.network.store(None);
        }

        if let Some(connection_monitor_handle) = self.connection_monitor_handle.take() {
            connection_monitor_handle.stop().await;
        }

        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["anemo"])
            .set(0);
    }
}

// Adapt MetricsCallbackMaker and MetricsResponseCallback to anemo.

impl SizedRequest for anemo::Request<Bytes> {
    fn size(&self) -> usize {
        self.body().len()
    }

    fn route(&self) -> String {
        self.route().to_string()
    }
}

impl SizedResponse for anemo::Response<Bytes> {
    fn size(&self) -> usize {
        self.body().len()
    }

    fn error_type(&self) -> Option<String> {
        if self.status().is_success() {
            None
        } else {
            Some(self.status().to_string())
        }
    }
}

impl MakeCallbackHandler for MetricsCallbackMaker {
    type Handler = MetricsResponseCallback;

    fn make_handler(&self, request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        self.handle_request(request)
    }
}

impl ResponseHandler for MetricsResponseCallback {
    fn on_response(mut self, response: &anemo::Response<bytes::Bytes>) {
        MetricsResponseCallback::on_response(&mut self, response)
    }

    fn on_error<E>(mut self, err: &E) {
        MetricsResponseCallback::on_error(&mut self, err)
    }
}

/// Network message types.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SendBlockRequest {
    // Serialized SignedBlock.
    block: Bytes,
}

#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub(crate) struct SendBlockResponse {}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchBlocksRequest {
    block_refs: Vec<Vec<u8>>,
    highest_accepted_rounds: Vec<Round>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchBlocksResponse {
    // Serialized SignedBlock.
    blocks: Vec<Bytes>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchCommitsRequest {
    start: CommitIndex,
    end: CommitIndex,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchCommitsResponse {
    // Serialized consecutive Commit.
    commits: Vec<Bytes>,
    // Serialized SignedBlock that certify the last commit from above.
    certifier_blocks: Vec<Bytes>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchLatestBlocksRequest {
    authorities: Vec<AuthorityIndex>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchLatestBlocksResponse {
    // Serialized SignedBlocks.
    blocks: Vec<Bytes>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct GetLatestRoundsRequest {}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct GetLatestRoundsResponse {
    // Highest received round per authority.
    highest_received: Vec<Round>,
    // Highest accepted round per authority.
    highest_accepted: Vec<Round>,
}
