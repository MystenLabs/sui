// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::Network;
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer};
use mysten_network::config::Config;
use prometheus::Registry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub mod api;
pub mod discovery;
pub mod state_sync;
pub mod utils;

use narwhal_network::metrics::{
    MetricsMakeCallbackHandler, NetworkConnectionMetrics, NetworkMetrics,
};
use sui_config::p2p::P2pConfig;
use sui_types::crypto::{KeypairTraits, NetworkKeyPair};
use sui_types::storage::{ReadStore, WriteStore};
pub use tonic;
use tower::ServiceBuilder;
use tracing::info;

pub const DEFAULT_CONNECT_TIMEOUT_SEC: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(30);
pub const DEFAULT_HTTP2_KEEPALIVE_SEC: Duration = Duration::from_secs(5);

pub fn default_mysten_network_config() -> Config {
    let mut net_config = Config::new();
    net_config.connect_timeout = Some(DEFAULT_CONNECT_TIMEOUT_SEC);
    net_config.request_timeout = Some(DEFAULT_REQUEST_TIMEOUT_SEC);
    net_config.http2_keepalive_interval = Some(DEFAULT_HTTP2_KEEPALIVE_SEC);
    net_config
}

pub fn create_p2p_network<S>(
    p2p_config: P2pConfig,
    state_sync_store: S,
    network_key_pair: NetworkKeyPair,
    prometheus_registry: &Registry,
) -> anyhow::Result<(Network, discovery::Handle, state_sync::Handle)>
where
    S: WriteStore + Clone + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    let (state_sync, state_sync_server) = state_sync::Builder::new()
        .config(config.p2p_config.state_sync.clone().unwrap_or_default())
        .store(state_sync_store)
        .with_metrics(prometheus_registry)
        .build();

    let (discovery, discovery_server) = discovery::Builder::new().config(p2p_config).build();

    let p2p_network = {
        let routes = anemo::Router::new()
            .add_rpc_service(discovery_server)
            .add_rpc_service(state_sync_server);

        let inbound_network_metrics = NetworkMetrics::new("sui", "inbound", prometheus_registry);
        let outbound_network_metrics = NetworkMetrics::new("sui", "outbound", prometheus_registry);
        let network_connection_metrics = NetworkConnectionMetrics::new("sui", prometheus_registry);

        let service = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                Arc::new(inbound_network_metrics),
            )))
            .service(routes);

        let outbound_layer = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_client_and_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                Arc::new(outbound_network_metrics),
            )))
            .into_inner();

        let mut anemo_config = config.p2p_config.anemo_config.clone().unwrap_or_default();
        if anemo_config.max_frame_size.is_none() {
            // Temporarily set a default size limit of 8 MiB for all RPCs. This helps us
            // catch bugs where size limits are missing from other parts of our code.
            // TODO: remove this and revert to default anemo max_frame_size once size
            // limits are fully implemented on sui data structures.
            anemo_config.max_frame_size = Some(8 << 20);
        }

        let network = Network::bind(config.p2p_config.listen_address)
            .server_name("sui")
            .private_key(network_key_pair.private().0.to_bytes())
            .config(anemo_config)
            .outbound_request_layer(outbound_layer)
            .start(service)?;
        info!("P2p network started on {}", network.local_addr());

        let _connection_monitor_handle = narwhal_network::connectivity::ConnectionMonitor::spawn(
            network.downgrade(),
            network_connection_metrics,
            HashMap::default(),
        );

        network
    };

    let discovery_handle = discovery.start(p2p_network.clone());
    let state_sync_handle = state_sync.start(p2p_network.clone());
    Ok((p2p_network, discovery_handle, state_sync_handle))
}
