// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, panic, sync::Arc, thread::sleep, time::Duration};

use anemo::{types::PeerInfo, PeerId, Response};
use anemo_tower::auth::{AllowedPeers, RequireAuthorizationLayer};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use fastcrypto::traits::KeyPair as _;
use tokio::sync::broadcast::error::RecvError;
use tracing::{error, warn};

use super::{
    anemo_gen::{
        consensus_rpc_client::ConsensusRpcClient,
        consensus_rpc_server::{ConsensusRpc, ConsensusRpcServer},
    },
    FetchBlocksRequest, FetchBlocksResponse, NetworkClient, NetworkManager, NetworkService,
    SendBlockRequest, SendBlockResponse,
};
use crate::{
    block::BlockRef,
    context::Context,
    error::{ConsensusError, ConsensusResult},
};

/// Implements RPC client for Consensus.
pub(crate) struct AnemoClient {
    context: Arc<Context>,
    network: Arc<ArcSwapOption<anemo::Network>>,
}

impl AnemoClient {
    const GET_CLIENT_INTERVAL: Duration = Duration::from_millis(10);
    const SEND_BLOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const FETCH_BLOCK_TIMEOUT: Duration = Duration::from_secs(15);

    #[allow(unused)]
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            network: Arc::new(ArcSwapOption::default()),
        }
    }

    pub(crate) fn set_network(&self, network: anemo::Network) {
        self.network.store(Some(Arc::new(network)));
    }

    async fn get_anemo_client(
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
        let peer_id = PeerId(authority.network_key.0.into());
        if let Some(peer) = network.peer(peer_id) {
            return Ok(ConsensusRpcClient::new(peer));
        };

        // If we're not connected we'll need to check to see if the Peer is a KnownPeer
        if network.known_peers().get(&peer_id).is_none() {
            return Err(ConsensusError::UnknownNetworkPeer(format!("{}", peer_id)));
        }

        let (mut subscriber, _) = network.subscribe().map_err(|e| {
            ConsensusError::NetworkError(format!("Cannot subscribe to AnemoNetwork updates: {e:?}"))
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
                        error!("Peer {} should be connected.", peer_id)
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
    async fn send_block(&self, peer: AuthorityIndex, block: &Bytes) -> ConsensusResult<()> {
        let mut client = self
            .get_anemo_client(peer, Self::SEND_BLOCK_TIMEOUT)
            .await?;
        let request = SendBlockRequest {
            block: block.clone(),
        };
        client
            .send_block(anemo::Request::new(request).with_timeout(Self::SEND_BLOCK_TIMEOUT))
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("{e:?}")))?;
        Ok(())
    }

    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self
            .get_anemo_client(peer, Self::FETCH_BLOCK_TIMEOUT)
            .await?;
        let request = FetchBlocksRequest { block_refs };
        let response = client
            .fetch_blocks(anemo::Request::new(request).with_timeout(Self::FETCH_BLOCK_TIMEOUT))
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("{e:?}")))?;
        Ok(response.into_body().blocks)
    }
}

/// Proxies Anemo RPC handlers to AnemoService.
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
                let peer_id = PeerId(authority.network_key.0.into());
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
        let index = self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let block = request.into_body().block;
        self.service
            .handle_send_block(*index, block)
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
        let index = self.peer_map.get(peer_id).ok_or_else(|| {
            anemo::rpc::Status::new_with_message(
                anemo::types::response::StatusCode::BadRequest,
                "peer not found",
            )
        })?;
        let block_refs = request.into_body().block_refs;
        let blocks = self
            .service
            .handle_fetch_blocks(*index, block_refs)
            .await
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    anemo::types::response::StatusCode::BadRequest,
                    format!("{e}"),
                )
            })?;
        Ok(Response::new(FetchBlocksResponse { blocks }))
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
    client: Arc<AnemoClient>,
    network: Arc<ArcSwapOption<anemo::Network>>,
}

impl AnemoManager {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context: context.clone(),
            client: Arc::new(AnemoClient::new(context)),
            network: Arc::new(ArcSwapOption::default()),
        }
    }
}

impl<S: NetworkService> NetworkManager<S> for AnemoManager {
    type Client = AnemoClient;

    fn new(context: Arc<Context>) -> Self {
        AnemoManager::new(context)
    }

    fn client(&self) -> Arc<Self::Client> {
        self.client.clone()
    }

    fn install_service(&self, network_keypair: NetworkKeyPair, service: Arc<S>) {
        let server = ConsensusRpcServer::new(AnemoServiceProxy::new(self.context.clone(), service));
        let authority = self.context.committee.authority(self.context.own_index);
        let address = authority.address.clone();
        let all_peer_ids = self
            .context
            .committee
            .authorities()
            .map(|(_i, authority)| PeerId(authority.network_key.0.to_bytes()));
        // TODO: add layers for metrics and additional filters.
        let routes = anemo::Router::new()
            .route_layer(RequireAuthorizationLayer::new(AllowedPeers::new(
                all_peer_ids,
            )))
            .add_rpc_service(server);
        let service = tower::ServiceBuilder::new().service(routes);

        // TODO: instrument with metrics and failpoints.

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
        let addr = address
            .to_anemo_address()
            .unwrap_or_else(|op| panic!("{op}: {address}"));
        let network = loop {
            let network_result = anemo::Network::bind(addr.clone())
                .server_name("consensus")
                .private_key(network_keypair.copy().private().0.to_bytes())
                .config(anemo_config.clone())
                // TODO: add outbound request layer
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
                    error!(
                        "Address {addr} should be available for the primary Narwhal service, retrying in one second: {e:#?}",
                    );
                    sleep(Duration::from_secs(1));
                }
            }
        };

        for (_i, authority) in self.context.committee.authorities() {
            let peer_id = PeerId(authority.network_key.0.to_bytes());
            let address = authority.address.to_anemo_address().unwrap();
            let peer_info = PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: vec![address.clone()],
            };
            network.known_peers().insert(peer_info);
        }

        self.client.set_network(network.clone());
        self.network.store(Some(Arc::new(network)));
    }

    async fn stop(&self) {
        if let Some(network) = self.network.load_full() {
            if let Err(e) = network.shutdown().await {
                warn!("Failure when shutting down AnemoNetwork: {e:?}");
            }
            self.network.store(None);
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use fastcrypto::traits::KeyPair;
    use parking_lot::Mutex;

    use crate::{
        block::BlockRef,
        context::Context,
        error::ConsensusResult,
        network::{anemo_network::AnemoManager, NetworkClient, NetworkManager, NetworkService},
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

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_basics() {
        let (context, keys) = Context::new_for_test(4);

        let context_0 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(0).unwrap()),
        );
        let manager_0 = AnemoManager::new(context_0.clone());
        let client_0 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_0);
        let service_0 = Arc::new(Mutex::new(TestService::new()));
        manager_0.install_service(keys[0].0.copy(), service_0.clone());

        let context_1 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(1).unwrap()),
        );
        let manager_1 = AnemoManager::new(context_1.clone());
        let client_1 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_1);
        let service_1 = Arc::new(Mutex::new(TestService::new()));
        manager_1.install_service(keys[1].0.copy(), service_1.clone());

        // Test that servers can receive client RPCs.
        client_0
            .send_block(
                context.committee.to_authority_index(1).unwrap(),
                &Bytes::from_static(b"msg 0"),
            )
            .await
            .unwrap();
        client_1
            .send_block(
                context.committee.to_authority_index(0).unwrap(),
                &Bytes::from_static(b"msg 1"),
            )
            .await
            .unwrap();
        assert_eq!(service_0.lock().handle_send_block.len(), 1);
        assert_eq!(service_0.lock().handle_send_block[0].0.value(), 1);
        assert_eq!(service_1.lock().handle_send_block.len(), 1);
        assert_eq!(service_1.lock().handle_send_block[0].0.value(), 0);

        // `Committee` is generated with the same random seed in Context::new_for_test(),
        // so the first 4 authorities are the same.
        let (context_4, keys_4) = Context::new_for_test(5);
        let context_4 = Arc::new(
            context_4
                .clone()
                .with_authority_index(context_4.committee.to_authority_index(4).unwrap()),
        );
        let manager_4 = AnemoManager::new(context_4.clone());
        let client_4 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_4);
        let service_4 = Arc::new(Mutex::new(TestService::new()));
        manager_4.install_service(keys_4[4].0.copy(), service_4.clone());

        // client_4 should not be able to reach service_0 or service_1, because of the
        // AllowedPeers filter.
        assert!(client_4
            .send_block(
                context.committee.to_authority_index(0).unwrap(),
                &Bytes::from_static(b"msg 2"),
            )
            .await
            .is_err());
        assert!(client_4
            .send_block(
                context.committee.to_authority_index(1).unwrap(),
                &Bytes::from_static(b"msg 3"),
            )
            .await
            .is_err());
    }
}
