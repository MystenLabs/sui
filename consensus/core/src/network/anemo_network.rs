// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, net::Ipv4Addr, sync::Arc, thread::sleep, time::Duration};

use anemo::Response;
use anemo_tower::auth::{AllowedPeers, RequireAuthorizationLayer};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use fastcrypto::traits::KeyPair as _;
use mysten_network::multiaddr::Protocol;
use tracing::error;

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
    ) -> ConsensusResult<ConsensusRpcClient<anemo::Peer>> {
        let network = loop {
            if let Some(network) = self.network.load_full() {
                break network;
            } else {
                tokio::time::sleep(Self::GET_CLIENT_INTERVAL).await;
            }
        };

        let authority = self.context.committee.authority(peer);
        let peer_id = anemo::PeerId(authority.network_key.0.into());
        let Some(peer) = network.peer(peer_id) else {
            return Err(ConsensusError::Disconnected(authority.hostname.clone()));
        };
        Ok(ConsensusRpcClient::new(peer))
    }
}

impl NetworkClient for AnemoClient {
    async fn send_block(&self, peer: AuthorityIndex, block: &Bytes) -> ConsensusResult<()> {
        let mut client = self.get_anemo_client(peer).await?;
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
        let mut client = self.get_anemo_client(peer).await?;
        let request = FetchBlocksRequest { block_refs };
        let response = client
            .fetch_blocks(anemo::Request::new(request).with_timeout(Self::FETCH_BLOCK_TIMEOUT))
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("{e:?}")))?;
        Ok(response.into_body().blocks)
    }
}

/// Proxies Anemo RPC handlers to AnemoService.
#[allow(unused)]
struct AnemoServiceProxy<S: NetworkService> {
    context: Arc<Context>,
    peer_map: BTreeMap<anemo::PeerId, AuthorityIndex>,
    service: Arc<S>,
}

impl<S: NetworkService> AnemoServiceProxy<S> {
    fn new(context: Arc<Context>, service: Arc<S>) -> Self {
        let peer_map = context
            .committee
            .authorities()
            .map(|(index, authority)| {
                let peer_id = anemo::PeerId(authority.network_key.0.into());
                (peer_id, index)
            })
            .collect();
        Self {
            context,
            peer_map,
            service,
        }
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
}

#[allow(unused)]
impl AnemoManager {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context: context.clone(),
            client: Arc::new(AnemoClient::new(context)),
        }
    }
}

impl<S: NetworkService> NetworkManager<AnemoClient, S> for AnemoManager {
    fn client(&self) -> Arc<AnemoClient> {
        self.client.clone()
    }

    fn install_service(&self, network_signer: NetworkKeyPair, service: Arc<S>) {
        let server = ConsensusRpcServer::new(AnemoServiceProxy::new(self.context.clone(), service));
        let authority = self.context.committee.authority(self.context.own_index);
        let address = authority
            .address
            .clone()
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let all_peer_ids = self
            .context
            .committee
            .authorities()
            .map(|(_i, authority)| anemo::PeerId(authority.network_key.0.to_bytes()));
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

        let network;
        let mut retries_left = 90;
        let addr = address.to_anemo_address().unwrap();
        loop {
            let network_result = anemo::Network::bind(addr.clone())
                .server_name("consensus")
                .private_key(network_signer.copy().private().0.to_bytes())
                .config(anemo_config.clone())
                // TODO: add outbound request layer
                .start(service.clone());
            match network_result {
                Ok(n) => {
                    network = n;
                    break;
                }
                Err(e) => {
                    retries_left -= 1;

                    if retries_left <= 0 {
                        panic!("Failed to initialize Network!");
                    }
                    error!(
                        "Address {addr} should be available for the primary Narwhal service, retrying in one second: {e:#?}",
                    );
                    sleep(Duration::from_secs(1));
                }
            }
        }

        self.client.set_network(network);
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::{Authority, AuthorityIndex, Committee, NetworkKeyPair, ProtocolKeyPair};
    use fastcrypto::traits::KeyPair;
    use parking_lot::Mutex;
    use rand::{rngs::StdRng, SeedableRng};

    use crate::{
        block::BlockRef,
        context::Context,
        error::ConsensusResult,
        network::{
            anemo_network::{AnemoClient, AnemoManager},
            NetworkClient, NetworkManager, NetworkService,
        },
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
    async fn test_basics() {
        let authorities_stake = vec![100, 100, 100, 100];
        let mut authorities = vec![];
        let mut key_pairs = vec![];
        let mut rng = StdRng::from_seed([0; 32]);
        for (i, stake) in authorities_stake.into_iter().enumerate() {
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            authorities.push(Authority {
                stake,
                address: format!("/ip4/127.0.0.1/udp/{}", 9090 + i).parse().unwrap(),
                hostname: format!("test_host {i}").to_string(),
                network_key: network_keypair.public().clone(),
                protocol_key: protocol_keypair.public().clone(),
            });
            key_pairs.push((network_keypair, protocol_keypair));
        }
        let committee = Committee::new(0, authorities);
        let (context, keys) = Context::new_for_test(4);

        let context_0 = Arc::new(
            context
                .clone()
                .with_committee(committee.clone())
                .with_authority_index(committee.to_authority_index(0).unwrap()),
        );
        let manager_0 = AnemoManager::new(context_0.clone());
        let client_0 =
            <AnemoManager as NetworkManager<AnemoClient, Mutex<TestService>>>::client(&manager_0);
        let service_0 = Arc::new(Mutex::new(TestService::new()));
        manager_0.install_service(keys[0].0.copy(), service_0.clone());

        let context_1 = Arc::new(
            context
                .clone()
                .with_committee(committee.clone())
                .with_authority_index(committee.to_authority_index(1).unwrap()),
        );
        let manager_1 = AnemoManager::new(context_1.clone());
        let client_1 =
            <AnemoManager as NetworkManager<AnemoClient, Mutex<TestService>>>::client(&manager_1);
        let service_1 = Arc::new(Mutex::new(TestService::new()));
        manager_0.install_service(keys[1].0.copy(), service_1.clone());

        client_0
            .send_block(
                committee.to_authority_index(1).unwrap(),
                &Bytes::from_static(b"msg 0"),
            )
            .await
            .unwrap();
        client_1
            .send_block(
                committee.to_authority_index(0).unwrap(),
                &Bytes::from_static(b"msg 1"),
            )
            .await
            .unwrap();
    }
}
