// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    panic,
    sync::Arc,
    time::Duration,
};

use anemo::{types::PeerInfo, PeerId, Response};
use anemo_tower::{
    auth::{AllowedPeers, RequireAuthorizationLayer},
    callback::{CallbackLayer, MakeCallbackHandler, ResponseHandler},
    set_header::{SetRequestHeaderLayer, SetResponseHeaderLayer},
    trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer},
};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use bytes::Bytes;
use cfg_if::cfg_if;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use prometheus::HistogramTimer;
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
    metrics::NetworkRouteMetrics,
    BlockStream, NetworkClient, NetworkManager, NetworkService,
};
use crate::{
    block::{BlockRef, VerifiedBlock},
    context::Context,
    error::{ConsensusError, ConsensusResult},
    Round,
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
            .map_err(|e| ConsensusError::NetworkError(format!("send_block failed: {e:?}")))?;
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
        };
        let response = client
            .fetch_blocks(anemo::Request::new(request).with_timeout(timeout))
            .await
            .map_err(|e| ConsensusError::NetworkError(format!("fetch_blocks failed: {e:?}")))?;
        Ok(response.into_body().blocks)
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
        let block_refs = request
            .into_body()
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
    connection_monitor_handle: Option<ConnectionMonitorHandle>,
}

impl AnemoManager {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context: context.clone(),
            client: Arc::new(AnemoClient::new(context)),
            network: Arc::new(ArcSwapOption::default()),
            connection_monitor_handle: None,
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

    async fn install_service(&mut self, network_keypair: NetworkKeyPair, service: Arc<S>) {
        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["anemo"])
            .set(1);

        let server = ConsensusRpcServer::new(AnemoServiceProxy::new(self.context.clone(), service));
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
        let epoch_string: String = self.context.committee.epoch().to_string();
        let inbound_network_metrics =
            Arc::new(self.context.metrics.network_metrics.inbound.clone());
        let outbound_network_metrics =
            Arc::new(self.context.metrics.network_metrics.outbound.clone());
        let quinn_connection_metrics = self.context.metrics.quinn_connection_metrics.clone();
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
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                inbound_network_metrics,
                self.context.parameters.anemo.excessive_message_size(),
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
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                outbound_network_metrics,
                self.context.parameters.anemo.excessive_message_size(),
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
        let private_key_bytes = network_keypair.private_key_bytes();
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
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FetchBlocksResponse {
    // Serialized SignedBlock.
    blocks: Vec<Bytes>,
}

#[derive(Clone)]
pub(crate) struct MetricsMakeCallbackHandler {
    metrics: Arc<NetworkRouteMetrics>,
    /// Size in bytes above which a request or response message is considered excessively large
    excessive_message_size: usize,
}

impl MetricsMakeCallbackHandler {
    pub fn new(metrics: Arc<NetworkRouteMetrics>, excessive_message_size: usize) -> Self {
        Self {
            metrics,
            excessive_message_size,
        }
    }
}

impl MakeCallbackHandler for MetricsMakeCallbackHandler {
    type Handler = MetricsResponseHandler;

    fn make_handler(&self, request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        let route = request.route().to_owned();

        self.metrics.requests.with_label_values(&[&route]).inc();
        self.metrics
            .inflight_requests
            .with_label_values(&[&route])
            .inc();
        let body_len = request.body().len();
        self.metrics
            .request_size
            .with_label_values(&[&route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large request with size {body_len} for {route} with peer {:?}",
                request.peer_id()
            );
            self.metrics
                .excessive_size_requests
                .with_label_values(&[&route])
                .inc();
        }

        let timer = self
            .metrics
            .request_latency
            .with_label_values(&[&route])
            .start_timer();

        MetricsResponseHandler {
            metrics: self.metrics.clone(),
            timer,
            route,
            excessive_message_size: self.excessive_message_size,
        }
    }
}

pub(crate) struct MetricsResponseHandler {
    metrics: Arc<NetworkRouteMetrics>,
    // The timer is held on to and "observed" once dropped
    #[allow(unused)]
    timer: HistogramTimer,
    route: String,
    excessive_message_size: usize,
}

impl ResponseHandler for MetricsResponseHandler {
    fn on_response(self, response: &anemo::Response<bytes::Bytes>) {
        let body_len = response.body().len();
        self.metrics
            .response_size
            .with_label_values(&[&self.route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large response with size {body_len} for {} with peer {:?}",
                self.route,
                response.peer_id()
            );
            self.metrics
                .excessive_size_responses
                .with_label_values(&[&self.route])
                .inc();
        }

        if !response.status().is_success() {
            let status = response.status().to_u16().to_string();
            self.metrics
                .errors
                .with_label_values(&[&self.route, &status])
                .inc();
        }
    }

    fn on_error<E>(self, _error: &E) {
        self.metrics
            .errors
            .with_label_values(&[&self.route, "unknown"])
            .inc();
    }
}

impl Drop for MetricsResponseHandler {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[&self.route])
            .dec();
    }
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use parking_lot::Mutex;
    use tokio::time::sleep;

    use crate::{
        block::{BlockRef, TestBlock, VerifiedBlock},
        context::Context,
        error::ConsensusResult,
        network::{
            anemo_network::AnemoManager, BlockStream, NetworkClient, NetworkManager, NetworkService,
        },
        Round,
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

        async fn handle_subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
        ) -> ConsensusResult<BlockStream> {
            unimplemented!()
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
    async fn anemo_send_block() {
        let (context, keys) = Context::new_for_test(4);

        let context_0 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(0).unwrap()),
        );
        let mut manager_0 = AnemoManager::new(context_0.clone());
        let client_0 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_0);
        let service_0 = Arc::new(Mutex::new(TestService::new()));
        manager_0
            .install_service(keys[0].0.clone(), service_0.clone())
            .await;

        let context_1 = Arc::new(
            context
                .clone()
                .with_authority_index(context.committee.to_authority_index(1).unwrap()),
        );
        let mut manager_1 = AnemoManager::new(context_1.clone());
        let client_1 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_1);
        let service_1 = Arc::new(Mutex::new(TestService::new()));
        manager_1
            .install_service(keys[1].0.clone(), service_1.clone())
            .await;

        // Wait for anemo to initialize.
        sleep(Duration::from_secs(5)).await;

        // Test that servers can receive client RPCs.
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

        // `Committee` is generated with the same random seed in Context::new_for_test(),
        // so the first 4 authorities are the same.
        let (context_4, keys_4) = Context::new_for_test(5);
        let context_4 = Arc::new(
            context_4
                .clone()
                .with_authority_index(context_4.committee.to_authority_index(4).unwrap()),
        );
        let mut manager_4 = AnemoManager::new(context_4.clone());
        let client_4 = <AnemoManager as NetworkManager<Mutex<TestService>>>::client(&manager_4);
        let service_4 = Arc::new(Mutex::new(TestService::new()));
        manager_4
            .install_service(keys_4[4].0.clone(), service_4.clone())
            .await;

        // client_4 should not be able to reach service_0 or service_1, because of the
        // AllowedPeers filter.
        let test_block_2 = VerifiedBlock::new_for_test(TestBlock::new(9, 2).build());
        assert!(client_4
            .send_block(
                context.committee.to_authority_index(0).unwrap(),
                &test_block_2,
                Duration::from_secs(5),
            )
            .await
            .is_err());
        let test_block_3 = VerifiedBlock::new_for_test(TestBlock::new(9, 3).build());
        assert!(client_4
            .send_block(
                context.committee.to_authority_index(1).unwrap(),
                &test_block_3,
                Duration::from_secs(5),
            )
            .await
            .is_err());
    }
}
