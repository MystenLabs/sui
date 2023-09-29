// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{RpcConnectionConfig, ServiceConfig};
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
use std::env;

pub async fn start_example_server(conn: RpcConnectionConfig, service_config: ServiceConfig) -> Result<(), crate::error::Error> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let sui_sdk_client_v0 = sui_sdk_client_v0(&conn.rpc_url).await;
    let data_loader = lru_cache_data_loader(&sui_sdk_client_v0).await;

    let pg_conn_pool: Option<PgManager> = match env::var("PG_DB_URL") {
        Ok(database_url) => {
            if database_url.is_empty() {
                println!("No DB URL provided, defaulting to SDK data provider");
                None
            } else {
                println!("DB url provided, setting DB as data provider");
                Some(PgManager::new(database_url, None)?)
            }
        }
        Err(_) => None
    };

    let data_provider: Box<dyn DataProvider> = match pg_conn_pool {
        Some(pg_conn_pool) => Box::new(pg_conn_pool),
        None => Box::new(sui_sdk_client_v0),
    };

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
