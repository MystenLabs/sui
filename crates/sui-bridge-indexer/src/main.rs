// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::*;
use tokio::task::JoinHandle;
use tracing::info;

use mysten_metrics::metered_channel::channel;
use mysten_metrics::spawn_logged_monitored_task;
use mysten_metrics::start_prometheus_server;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metered_eth_provider::MeteredEthHttpProvier;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge_indexer::config::IndexerConfig;
use sui_bridge_indexer::eth_worker::EthBridgeWorker;
use sui_bridge_indexer::indexer_builder::{
    IndexerBuilder, SuiCheckpointDatasource, SuiInputObjectFilter,
};
use sui_bridge_indexer::metrics::BridgeIndexerMetrics;
use sui_bridge_indexer::postgres_manager::{get_connection_pool, read_sui_progress_store};
use sui_bridge_indexer::sui_bridge_indexer::{PgBridgePersistent, SuiBridgeDataMapper};
use sui_bridge_indexer::sui_transaction_handler::handle_sui_transactions_loop;
use sui_bridge_indexer::sui_transaction_queries::start_sui_tx_polling_task;
use sui_config::Config;
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_sdk::SuiClientBuilder;
use sui_types::SUI_BRIDGE_OBJECT_ID;

#[derive(Parser, Clone, Debug)]
struct Args {
    /// Path to a yaml config
    #[clap(long, short)]
    config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    // load config
    let config_path = if let Some(path) = args.config_path {
        path
    } else {
        env::current_dir()
            .expect("Couldn't get current directory")
            .join("config.yaml")
    };
    let config = IndexerConfig::load(&config_path)?;

    // Init metrics server
    let registry_service = start_prometheus_server(
        format!("{}:{}", config.metric_url, config.metric_port,)
            .parse()
            .unwrap_or_else(|err| panic!("Failed to parse metric address: {}", err)),
    );
    let registry = registry_service.default_registry();

    mysten_metrics::init_metrics(&registry);

    info!(
        "Metrics server started at {}::{}",
        config.metric_url, config.metric_port
    );
    let indexer_meterics = BridgeIndexerMetrics::new(&registry);
    let ingestion_metrics = DataIngestionMetrics::new(&registry);
    let bridge_metrics = Arc::new(BridgeMetrics::new(&registry));

    // unwrap safe: db_url must be set in `load_config` above
    let db_url = config.db_url.clone();

    // TODO: retry_with_max_elapsed_time
    let eth_worker = EthBridgeWorker::new(
        get_connection_pool(db_url.clone()),
        bridge_metrics.clone(),
        indexer_meterics.clone(),
        config.clone(),
    )?;

    let eth_client = Arc::new(
        EthClient::<MeteredEthHttpProvier>::new(
            &config.eth_rpc_url,
            HashSet::from_iter(vec![eth_worker.bridge_address()]),
            bridge_metrics.clone(),
        )
        .await?,
    );

    let unfinalized_handle = eth_worker
        .start_indexing_unfinalized_events(eth_client.clone())
        .await?;
    let finalized_handle = eth_worker
        .start_indexing_finalized_events(eth_client.clone())
        .await?;
    let handles = vec![unfinalized_handle, finalized_handle];

    if let Some(sui_rpc_url) = config.sui_rpc_url.clone() {
        start_processing_sui_checkpoints_by_querying_txns(
            sui_rpc_url,
            db_url.clone(),
            indexer_meterics.clone(),
            bridge_metrics,
        )
        .await?;
    } else {
        let sui_checkpoint_datasource = SuiCheckpointDatasource::new(
            config.remote_store_url,
            config.concurrency as usize,
            config.checkpoints_path.clone().into(),
            ingestion_metrics.clone(),
        );
        let indexer = IndexerBuilder::new(
            sui_checkpoint_datasource,
            SuiInputObjectFilter {
                object_id: SUI_BRIDGE_OBJECT_ID,
            },
            SuiBridgeDataMapper,
        )
        .build(PgBridgePersistent::new(
            get_connection_pool(db_url.clone()),
            config.bridge_genesis_checkpoint,
        ));
        indexer.start().await?;
    }
    // We are not waiting for the sui tasks to finish here, which is ok.
    futures::future::join_all(handles).await;

    Ok(())
}

async fn start_processing_sui_checkpoints_by_querying_txns(
    sui_rpc_url: String,
    db_url: String,
    indexer_metrics: BridgeIndexerMetrics,
    bridge_metrics: Arc<BridgeMetrics>,
) -> Result<Vec<JoinHandle<()>>> {
    let pg_pool = get_connection_pool(db_url.clone());
    let (tx, rx) = channel(
        100,
        &mysten_metrics::get_metrics()
            .unwrap()
            .channel_inflight
            .with_label_values(&["sui_transaction_processing_queue"]),
    );
    let mut handles = vec![];
    let cursor =
        read_sui_progress_store(&pg_pool).expect("Failed to read cursor from sui progress store");
    let sui_client = SuiClientBuilder::default().build(sui_rpc_url).await?;
    handles.push(spawn_logged_monitored_task!(
        start_sui_tx_polling_task(sui_client, cursor, tx, bridge_metrics),
        "start_sui_tx_polling_task"
    ));
    handles.push(spawn_logged_monitored_task!(
        handle_sui_transactions_loop(pg_pool.clone(), rx, indexer_metrics.clone()),
        "handle_sui_transcations_loop"
    ));
    Ok(handles)
}
