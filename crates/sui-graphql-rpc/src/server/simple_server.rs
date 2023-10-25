// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{ConnectionConfig, ServiceConfig};
use crate::context_data::db_data_provider::PgManager;
use crate::context_data::package_cache::PackageCache;
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

    // TODO (wlmyng): Allow users to choose which data sources to back graphql
    let reader = PgManager::reader(conn.db_url).expect("Failed to create pg connection pool");
    let pg_conn_pool = PgManager::new(reader.clone(), service_config.limits);
    let package_cache = PackageCache::new(reader);
    let name_service_config = NameServiceConfig::default();

    let prom_addr: SocketAddr = PROM_ADDR.parse().unwrap();
    let registry = start_prom(prom_addr);
    let metrics = RequestMetrics::new(&registry);

    let builder = ServerBuilder::new(conn.port, conn.host);
    println!("Launch GraphiQL IDE at: http://{}", builder.address());

    builder
        .max_query_depth(service_config.limits.max_query_depth)
        .max_query_nodes(service_config.limits.max_query_nodes)
        .context_data(service_config)
        .context_data(pg_conn_pool)
        .context_data(package_cache)
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
