// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use tracing::{error, info};

use sui_indexer::db::{get_pg_pool_connection, new_pg_connection_pool, reset_database};
use sui_indexer::errors::IndexerError;
use sui_indexer::indexer::Indexer;
use sui_indexer::metrics::start_prometheus_server;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::store::PgIndexerAnalyticalStore;
use sui_indexer::store::PgIndexerStore;
use sui_indexer::IndexerConfig;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let indexer_config = IndexerConfig::parse();
    info!("Parsed indexer config: {:#?}", indexer_config);

    let db_url = indexer_config.get_db_url().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed parsing database url with error {:?}",
            e
        ))
    })?;
    let blocking_cp = new_pg_connection_pool(&db_url, None).map_err(|e| {
        error!(
            "Failed creating Postgres connection pool with error {:?}",
            e
        );
        e
    })?;
    if indexer_config.reset_db {
        let mut conn = get_pg_pool_connection(&blocking_cp).map_err(|e| {
            error!(
                "Failed getting Postgres connection from connection pool with error {:?}",
                e
            );
            e
        })?;
        reset_database(&mut conn, /* drop_all */ true).map_err(|e| {
            let db_err_msg = format!(
                "Failed resetting database with url: {:?} and error: {:?}",
                db_url, e
            );
            error!("{}", db_err_msg);
            IndexerError::PostgresResetError(db_err_msg)
        })?;
    }

    let (_registry_service, registry) = start_prometheus_server(
        // NOTE: this parses the input host addr and port number for socket addr,
        // so unwrap() is safe here.
        format!(
            "{}:{}",
            indexer_config.client_metric_host, indexer_config.client_metric_port
        )
        .parse()
        .unwrap(),
        indexer_config.rpc_client_url.as_str(),
    )?;
    let indexer_metrics = IndexerMetrics::new(&registry);
    mysten_metrics::init_metrics(&registry);

    let report_cp = blocking_cp.clone();
    let report_metrics = indexer_metrics.clone();
    tokio::spawn(async move {
        loop {
            let cp_state = report_cp.state();
            info!(
                "DB connection pool size: {}, with idle conn: {}.",
                cp_state.connections, cp_state.idle_connections
            );
            report_metrics
                .db_conn_pool_size
                .set(cp_state.connections as i64);
            report_metrics
                .idle_db_conn
                .set(cp_state.idle_connections as i64);
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });

    if indexer_config.fullnode_sync_worker {
        let store = PgIndexerStore::new(blocking_cp, indexer_metrics.clone());
        return Indexer::start_writer(&indexer_config, store, indexer_metrics).await;
    } else if indexer_config.rpc_server_worker {
        return Indexer::start_reader(&indexer_config, &registry, db_url).await;
    } else if indexer_config.analytical_worker {
        let store = PgIndexerAnalyticalStore::new(blocking_cp);
        return Indexer::start_analytical_worker(store, indexer_metrics.clone()).await;
    }
    Ok(())
}
