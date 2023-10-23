// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{ConnectionConfig, ServiceConfig};
use crate::context_data::data_provider::DataProvider;
use crate::context_data::db_data_provider::PgManager;
use crate::context_data::sui_sdk_data_provider::{lru_cache_data_loader, sui_sdk_client_v0};
use crate::extensions::feature_gate::FeatureGate;
use crate::extensions::logger::Logger;
use crate::extensions::query_limits_checker::QueryLimitsChecker;
use crate::extensions::timeout::Timeout;
use crate::metrics::RequestMetrics;
use crate::server::builder::ServerBuilder;

use prometheus::Registry;
use std::default::Default;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_json_rpc::name_service::NameServiceConfig;

static PROM_ADDR: &str = "0.0.0.0:9184";

pub async fn start_example_server(
    conn: ConnectionConfig,
    service_config: ServiceConfig,
) -> Result<(), crate::error::Error> {
    println!("Starting server with config: {:?}", conn);
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let sui_sdk_client_v0 = sui_sdk_client_v0(&conn.rpc_url).await;
    let data_provider: Box<dyn DataProvider> = Box::new(sui_sdk_client_v0.clone());
    let data_loader = lru_cache_data_loader(&sui_sdk_client_v0).await;

    // TODO (wlmyng): Allow users to choose which data sources to back graphql
    let db_url = conn.db_url;
    let pg_conn_pool = PgManager::new(db_url, None)
        .map_err(|e| {
            println!("Failed to create pg connection pool: {}", e);
            e
        })
        .unwrap();
    let name_service_config = NameServiceConfig::default();

    let prom_addr: SocketAddr = PROM_ADDR.parse().unwrap();
    let registry = start_prom(prom_addr);
    let metrics = RequestMetrics::new(&registry);

    let builder = ServerBuilder::new(conn.port, conn.host);
    println!("Launch GraphiQL IDE at: http://{}", builder.address());

    builder
        .max_query_depth(service_config.limits.max_query_depth)
        .max_query_nodes(service_config.limits.max_query_nodes)
        .context_data(data_provider)
        .context_data(data_loader)
        .context_data(service_config)
        .context_data(pg_conn_pool)
        .context_data(name_service_config)
        .context_data(Arc::new(metrics))
        .extension(QueryLimitsChecker::default())
        .extension(FeatureGate)
        .extension(Logger::default())
        .extension(Timeout::default())
        .build()?
        .run()
        .await
}

fn start_prom(binding_address: SocketAddr) -> Registry {
    println!("Starting Prometheus HTTP endpoint at {}", binding_address);
    let registry_service = mysten_metrics::start_prometheus_server(binding_address);
    registry_service.default_registry()
}
