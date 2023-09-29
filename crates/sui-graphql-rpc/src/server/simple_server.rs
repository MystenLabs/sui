// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{ConnectionConfig, DataSourceConfig, RpcConfig, ServiceConfig};
use crate::context_data::data_provider::DataProvider;
use crate::context_data::db_data_provider::PgManager;
use crate::context_data::sui_sdk_data_provider::{lru_cache_data_loader, sui_sdk_client_v0};
use crate::extensions::feature_gate::FeatureGate;
use crate::extensions::limits_info::LimitsInfo;
use crate::extensions::logger::Logger;
use crate::extensions::query_limits_checker::QueryLimitsChecker;
use crate::extensions::timeout::Timeout;
use crate::server::builder::ServerBuilder;

use std::default::Default;

pub async fn start_example_server(
    conn: ConnectionConfig,
    datasource_config: DataSourceConfig,
    service_config: ServiceConfig,
) -> Result<(), crate::error::Error> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let data_provider: Box<dyn DataProvider> = match datasource_config {
        DataSourceConfig::Db(db_conn_config) => {
            println!("Configuring DB as data provider");
            let pg_conn_pool = PgManager::new(db_conn_config.db_url, Some(db_conn_config.config))?;
            Box::new(pg_conn_pool)
        }
        DataSourceConfig::Rpc(rpc_conn_config) => {
            println!("Configuring RPC as data provider");
            let sui_sdk_client_v0 = sui_sdk_client_v0(&rpc_conn_config.rpc_url).await;
            Box::new(sui_sdk_client_v0)
        }
    };

    // TODO (wlmyng): Hook this up w/ db once the queries are implemented
    let rpc_conn = RpcConfig::default();
    let sui_sdk_client_v0 = sui_sdk_client_v0(&rpc_conn.rpc_url).await;
    let data_loader = lru_cache_data_loader(&sui_sdk_client_v0).await;

    let builder = ServerBuilder::new(conn.port, conn.host);
    println!("Launch GraphiQL IDE at: http://{}", builder.address());

    builder
        .max_query_depth(service_config.limits.max_query_depth)
        .max_query_nodes(service_config.limits.max_query_nodes)
        .context_data(data_provider)
        .context_data(data_loader)
        .context_data(service_config)
        .extension(QueryLimitsChecker)
        .extension(FeatureGate)
        .extension(LimitsInfo)
        .extension(Logger::default())
        .extension(Timeout::default())
        .build()
        .run()
        .await;
    Ok(())
}
