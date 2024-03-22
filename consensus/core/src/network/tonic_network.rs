// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    net::{SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use cfg_if::cfg_if;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use mysten_network::{multiaddr::Protocol, Multiaddr};
use parking_lot::RwLock;
use tokio::{
    sync::oneshot::{self, Sender},
    task::JoinSet,
};
use tonic::{
    transport::{Channel, Server},
    Request, Response,
};
use tracing::{debug, info, warn};

use super::{
    tonic_gen::{
        consensus_service_client::ConsensusServiceClient,
        consensus_service_server::ConsensusService,
    },
    FetchBlocksRequest, FetchBlocksResponse, NetworkClient, NetworkManager, NetworkService,
    SendBlockRequest, SendBlockResponse,
};
use crate::{
    block::{BlockRef, VerifiedBlock},
    context::Context,
    error::{ConsensusError, ConsensusResult},
    network::tonic_gen::consensus_service_server::ConsensusServiceServer,
};

const AUTHORITY_INDEX_METADATA_KEY: &str = "authority-index";

// Implements Tonic RPC client for Consensus.
pub(crate) struct TonicClient {
    context: Arc<Context>,
    channel_pool: Arc<ChannelPool>,
}

impl TonicClient {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context: context.clone(),
            channel_pool: Arc::new(ChannelPool::new(context)),
        }
    }

    async fn get_client(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<ConsensusServiceClient<Channel>> {
        let channel = self.channel_pool.get_channel(peer, timeout).await?;
        Ok(ConsensusServiceClient::new(channel))
    }
}

#[async_trait]
impl NetworkClient for TonicClient {
    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(SendBlockRequest {
            block: block.serialized().clone(),
        });
        request.set_timeout(timeout);
        // TODO: remove below after adding authentication.
        request.metadata_mut().insert(
            AUTHORITY_INDEX_METADATA_KEY,
            self.context.own_index.value().to_string().parse().unwrap(),
        );
        client
            .send_block(request)
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("send_block failed: {e:?}")))?;
        Ok(())
    }

    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
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
        });
        request.set_timeout(timeout);
        // TODO: remove below after adding authentication.
        request.metadata_mut().insert(
            AUTHORITY_INDEX_METADATA_KEY,
            self.context.own_index.value().to_string().parse().unwrap(),
        );
        let response = client
            .fetch_blocks(request)
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("fetch_blocks failed: {e:?}")))?;
        Ok(response.into_inner().blocks)
    }
}

/// Manages a pool of connections to peers to avoid constantly reconnecting,
/// which can be expensive.
struct ChannelPool {
    context: Arc<Context>,
    // Size is limited by known authorities in the committee.
    channels: RwLock<BTreeMap<AuthorityIndex, Channel>>,
}

impl ChannelPool {
    fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            channels: RwLock::new(BTreeMap::new()),
        }
    }

    async fn get_channel(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<Channel> {
        {
            let channels = self.channels.read();
            if let Some(channel) = channels.get(&peer) {
                return Ok(channel.clone());
            }
        }

        let authority = self.context.committee.authority(peer);
        let address = to_host_port_str(&authority.address).map_err(|e| {
            ConsensusError::NetworkError(format!("Cannot convert address to host:port: {e:?}"))
        })?;
        let address = format!("http://{address}");
        let endpoint = Channel::from_shared(address.clone())
            .unwrap()
            .connect_timeout(timeout)
            .initial_connection_window_size(64 << 20)
            .initial_stream_window_size(32 << 20)
            .buffer_size(64 << 20);
        // TODO: tune endpoint options and set TLS config.

        let deadline = tokio::time::Instant::now() + timeout;
        let channel = loop {
            match endpoint.connect().await {
                Ok(channel) => break channel,
                Err(e) => {
                    warn!("Timed out connecting to endpoint at {address}: {e:?}");
                    if tokio::time::Instant::now() >= deadline {
                        return Err(ConsensusError::NetworkError(format!(
                            "Timed out connecting to endpoint at {address}: {e:?}"
                        )));
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };

        let mut channels = self.channels.write();
        // There should not be many concurrent attempts at connecting to the same peer.
        let channel = channels.entry(peer).or_insert(channel);
        Ok(channel.clone())
    }
}

/// Proxies Tonic requests to NetworkService with actual handler implementation.
struct TonicServiceProxy<S: NetworkService> {
    context: Arc<Context>,
    service: Arc<S>,
}

impl<S: NetworkService> TonicServiceProxy<S> {
    fn new(context: Arc<Context>, service: Arc<S>) -> Self {
        Self { context, service }
    }
}

#[async_trait]
impl<S: NetworkService> ConsensusService for TonicServiceProxy<S> {
    async fn send_block(
        &self,
        request: Request<SendBlockRequest>,
    ) -> Result<Response<SendBlockResponse>, tonic::Status> {
        // TODO: switch to using authenticated peer identity.
        let Some(peer_index) = request
            .metadata()
            .get(AUTHORITY_INDEX_METADATA_KEY)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
            .and_then(|index| self.context.committee.to_authority_index(index))
        else {
            return Err(tonic::Status::invalid_argument("Invalid authority index"));
        };
        let block = request.into_inner().block;
        self.service
            .handle_send_block(peer_index, block)
            .await
            .map_err(|e| tonic::Status::invalid_argument(format!("{e:?}")))?;
        Ok(Response::new(SendBlockResponse {}))
    }

    async fn fetch_blocks(
        &self,
        request: Request<FetchBlocksRequest>,
    ) -> Result<Response<FetchBlocksResponse>, tonic::Status> {
        // TODO: switch to using authenticated peer identity.
        let Some(peer_index) = request
            .metadata()
            .get(AUTHORITY_INDEX_METADATA_KEY)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
            .and_then(|index| self.context.committee.to_authority_index(index))
        else {
            return Err(tonic::Status::invalid_argument("Invalid authority index"));
        };
        let block_refs = request
            .into_inner()
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
        let blocks = self
            .service
            .handle_fetch_blocks(peer_index, block_refs)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        Ok(Response::new(FetchBlocksResponse { blocks }))
    }
}

/// Manages the lifecycle of Tonic network client and service. Typical usage during initialization:
/// 1. Create a new `TonicManager`.
/// 2. Take `TonicClient` from `TonicManager::client()`.
/// 3. Create consensus components.
/// 4. Create `TonicService` for consensus service handler.
/// 5. Install `TonicService` to `TonicManager` with `TonicManager::install_service()`.
pub(crate) struct TonicManager {
    context: Arc<Context>,
    client: Arc<TonicClient>,
    server: JoinSet<()>,
    shutdown: Option<Sender<()>>,
}

impl TonicManager {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context: context.clone(),
            client: Arc::new(TonicClient::new(context)),
            server: JoinSet::new(),
            shutdown: None,
        }
    }
}

impl<S: NetworkService> NetworkManager<S> for TonicManager {
    type Client = TonicClient;

    fn new(context: Arc<Context>) -> Self {
        TonicManager::new(context)
    }

    fn client(&self) -> Arc<Self::Client> {
        self.client.clone()
    }

    async fn install_service(&mut self, _network_keypair: NetworkKeyPair, service: Arc<S>) {
        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["tonic"])
            .set(1);

        let authority = self.context.committee.authority(self.context.own_index);
        // Bind to localhost in unit tests since only local networking is needed.
        // Bind to the unspecified address to allow the actual address to be assigned,
        // in simtest and production.
        cfg_if!(
            if #[cfg(test)] {
                let own_address = authority.address.with_localhost_ip();
            } else {
                let own_address = authority.address.with_zero_ip();
            }
        );
        let own_address = to_socket_addr(&own_address).unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown = Some(tx);
        let service = TonicServiceProxy::new(self.context.clone(), service);

        let server = Server::builder()
            .initial_connection_window_size(64 << 20)
            .initial_stream_window_size(32 << 20)
            .add_service(ConsensusServiceServer::new(service))
            .serve_with_shutdown(own_address, async move {
                match rx.await {
                    Ok(()) => {
                        debug!("Consensus tonic server is shutting down");
                    }
                    Err(e) => {
                        warn!("Consensus tonic server is shutting down at {own_address}: {e:?}");
                    }
                }
            });

        self.server.spawn(async move {
            if let Err(e) = server.await {
                warn!("TonicNetwork server failed: {e:?}");
            } else {
                info!("TonicNetwork server stopped");
            }
        });

        info!("TonicNetwork server started at: {own_address}");
    }

    async fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.server.join_next().await;

        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["tonic"])
            .set(0);
    }
}

/// Attempts to convert a multiaddr of the form `/[ip4,ip6,dns]/{}/udp/{port}` into
/// a host:port string.
fn to_host_port_str(addr: &Multiaddr) -> Result<String, &'static str> {
    let mut iter = addr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", ipaddr, port))
        }
        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", ipaddr, port))
        }
        (Some(Protocol::Dns(hostname)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", hostname, port))
        }

        _ => {
            tracing::warn!("unsupported multiaddr: '{addr}'");
            Err("invalid address")
        }
    }
}

/// Attempts to convert a multiaddr of the form `/[ip4,ip6]/{}/[udp,tcp]/{port}` into
/// a SocketAddr value.
fn to_socket_addr(addr: &Multiaddr) -> Result<SocketAddr, &'static str> {
    let mut iter = addr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port)))
        | (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Tcp(port))) => {
            Ok(SocketAddr::V4(SocketAddrV4::new(ipaddr, port)))
        }

        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port)))
        | (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Tcp(port))) => {
            Ok(SocketAddr::V6(SocketAddrV6::new(ipaddr, port, 0, 0)))
        }

        _ => {
            tracing::warn!("unsupported multiaddr: '{addr}'");
            Err("invalid address")
        }
    }
}

// TODO: after supporting peer authentication, using rtest to share the test case with anemo_network.rs
#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use parking_lot::Mutex;

    use crate::{
        block::{BlockRef, TestBlock, VerifiedBlock},
        context::Context,
        error::ConsensusResult,
        network::{tonic_network::TonicManager, NetworkClient, NetworkManager, NetworkService},
    };

    struct TestService {
        handle_send_block: Vec<(AuthorityIndex, Bytes)>,
        handle_fetch_blocks: Vec<(AuthorityIndex, Vec<BlockRef>)>,
    }

    impl TestService {
        pub(crate) fn new() -> Self {
            Self {
                handle_send_block: Vec::new(),
                handle_fetch_blocks: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl NetworkService for Mutex<TestService> {
        async fn handle_send_block(
            &self,
            peer: AuthorityIndex,
            block: Bytes,
        ) -> ConsensusResult<()> {
            self.lock().handle_send_block.push((peer, block));
            Ok(())
        }

        async fn handle_fetch_blocks(
            &self,
            peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
        ) -> ConsensusResult<Vec<Bytes>> {
            self.lock().handle_fetch_blocks.push((peer, block_refs));
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn tonic_basics() {
        let (context, keys) = Context::new_for_test(4);

        let context_0 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(0).unwrap()),
        );
        let mut manager_0 = TonicManager::new(context_0.clone());
        let client_0 = <TonicManager as NetworkManager<Mutex<TestService>>>::client(&manager_0);
        let service_0 = Arc::new(Mutex::new(TestService::new()));
        manager_0
            .install_service(keys[0].0.clone(), service_0.clone())
            .await;

        let context_1 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(1).unwrap()),
        );
        let mut manager_1 = TonicManager::new(context_1.clone());
        let client_1 = <TonicManager as NetworkManager<Mutex<TestService>>>::client(&manager_1);
        let service_1 = Arc::new(Mutex::new(TestService::new()));
        manager_1
            .install_service(keys[1].0.clone(), service_1.clone())
            .await;

        // Test that servers can receive client RPCs.
        // If the test uses simulated time, more retries will be necessary to make sure
        // the server is ready.
        let test_block_0 = VerifiedBlock::new_for_test(TestBlock::new(9, 0).build());
        client_0
            .send_block(
                context.committee.to_authority_index(1).unwrap(),
                &test_block_0,
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        let test_block_1 = VerifiedBlock::new_for_test(TestBlock::new(9, 1).build());
        client_1
            .send_block(
                context.committee.to_authority_index(0).unwrap(),
                &test_block_1,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

        assert_eq!(service_0.lock().handle_send_block.len(), 1);
        assert_eq!(service_0.lock().handle_send_block[0].0.value(), 1);
        assert_eq!(
            service_0.lock().handle_send_block[0].1,
            test_block_1.serialized(),
        );
        assert_eq!(service_1.lock().handle_send_block.len(), 1);
        assert_eq!(service_1.lock().handle_send_block[0].0.value(), 0);
        assert_eq!(
            service_1.lock().handle_send_block[0].1,
            test_block_0.serialized(),
        );
    }
}
