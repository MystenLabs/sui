// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::*;
use mysten_metrics::spawn_logged_monitored_task;
use mysten_metrics::start_prometheus_server;
use prometheus::Registry;
use tokio::task::JoinHandle;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge_indexer::eth_worker::EthBridgeWorker;
use sui_bridge_indexer::postgres_manager::{read_sui_progress_store, get_connection_pool, PgProgressStore};
use sui_bridge_indexer::sui_transaction_handler::handle_sui_transcations_loop;
use sui_bridge_indexer::sui_transaction_queries::start_sui_tx_polling_task;
use sui_bridge_indexer::sui_worker::SuiBridgeWorker;
use sui_bridge_indexer::{config::load_config, metrics::BridgeIndexerMetrics};
use sui_data_ingestion_core::{DataIngestionMetrics, IndexerExecutor, ReaderOptions, WorkerPool};
use sui_sdk::SuiClientBuilder;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use tokio::sync::oneshot;
use tracing::info;

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
    let config = load_config(&config_path).unwrap();
    let config_clone = config.clone();

    // Init metrics server
    let registry_service = start_prometheus_server(
        format!("{}:{}", config.metric_url, config.metric_port,)
            .parse()
            .unwrap_or_else(|err| panic!("Failed to parse metric address: {}", err)),
    );
    let registry: Registry = registry_service.default_registry();

    mysten_metrics::init_metrics(&registry);

    info!(
        "Metrics server started at {}::{}",
        config.metric_url, config.metric_port
    );
    let indexer_meterics = BridgeIndexerMetrics::new(&registry);
    let ingestion_metrics = DataIngestionMetrics::new(&registry);
    let bridge_metrics = Arc::new(BridgeMetrics::new(&registry));

    // unwrap safe: db_url must be set in `load_config` above
    let db_url = config.db_url.clone().unwrap();

    // TODO: retry_with_max_elapsed_time
    let eth_worker = EthBridgeWorker::new(
        get_connection_pool(db_url.clone()),
        indexer_meterics.clone(),
        config.clone(),
    )
    .unwrap();

    let eth_client = Arc::new(
        EthClient::<ethers::providers::Http>::new(
            &config.eth_rpc_url,
            HashSet::from_iter(vec![eth_worker.bridge_address()]),
            bridge_metrics,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );

    let unfinalized_handle = eth_worker.start_indexing_unfinalized_events(eth_client.clone()).await.unwrap();
    let finalized_handle = eth_worker.start_indexing_finalized_events(eth_client.clone()).await.unwrap();
    let handles = vec![unfinalized_handle, finalized_handle];
    // TODO: add retry_with_max_elapsed_time
    if let Some(sui_rpc_url) = config.sui_rpc_url.clone() {
        start_processing_sui_checkpoints_by_querying_txes(
            sui_rpc_url,
            db_url.clone(),
            indexer_meterics.clone(),
        ).await.unwrap();
    } else {
        let _ = start_processing_sui_checkpoints(
            &config_clone,
            db_url,
            indexer_meterics,
            ingestion_metrics,
        ).await;
    }

    let _ = futures::future::join_all(handles).await;

    Ok(())
}

async fn start_processing_sui_checkpoints(
    config: &sui_bridge_indexer::config::Config,
    db_url: String,
    indexer_meterics: BridgeIndexerMetrics,
    ingestion_metrics: DataIngestionMetrics,
) -> Result<HashMap<String, CheckpointSequenceNumber>> {
    // metrics init
    let (_exit_sender, exit_receiver) = oneshot::channel();

    let pg_pool = get_connection_pool(db_url.clone());
    let progress_store = PgProgressStore::new(pg_pool, config.bridge_genesis_checkpoint);
    let mut executor = IndexerExecutor::new(
        progress_store,
        1, /* workflow types */
        ingestion_metrics,
    );

    let indexer_metrics_cloned = indexer_meterics.clone();

    let worker_pool = WorkerPool::new(
        SuiBridgeWorker::new(vec![], db_url, indexer_metrics_cloned),
        "bridge worker".into(),
        config.concurrency as usize,
    );
    executor.register(worker_pool).await?;
    executor
        .run(
            config.checkpoints_path.clone().into(),
            Some(config.remote_store_url.clone()),
            vec![], // optional remote store access options
            ReaderOptions::default(),
            exit_receiver,
        )
        .await
}

async fn start_processing_sui_checkpoints_by_querying_txes(
    sui_rpc_url: String,
    db_url: String,
    indexer_meterics: BridgeIndexerMetrics,
) -> Result<Vec<JoinHandle<()>>> {
    // metrics init

    let pg_pool = get_connection_pool(db_url.clone());
    let (tx, rx) = mysten_metrics::metered_channel::channel(
        100,
        &mysten_metrics::get_metrics()
        .unwrap()
        .channel_inflight
        .with_label_values(&["sui_transaction_processing_queue"]),
    );
    let mut handles = vec![];
    // FIXME cursor
    let cursor = read_sui_progress_store(&pg_pool).unwrap();
    let sui_client = SuiClientBuilder::default().build(sui_rpc_url).await?;
    handles.push(spawn_logged_monitored_task!(
        start_sui_tx_polling_task(sui_client, cursor, tx),
        "start_sui_tx_polling_task"
    ));
    handles.push(spawn_logged_monitored_task!(
        handle_sui_transcations_loop(pg_pool.clone(), rx, indexer_meterics.clone(),),
        "handle_sui_transcations_loop"
    ));
    Ok(handles)
    // let (_exit_sender, exit_receiver) = oneshot::channel();

    // let progress_store = PgProgressStore::new(pg_pool, config.bridge_genesis_checkpoint);
    // let mut executor = IndexerExecutor::new(
    //     progress_store,
    //     1, /* workflow types */
    //     ingestion_metrics,
    // );

    // let indexer_metrics_cloned = indexer_meterics.clone();

    // let worker_pool = WorkerPool::new(
    //     SuiBridgeWorker::new(vec![], db_url, indexer_metrics_cloned),
    //     "bridge worker".into(),
    //     config.concurrency as usize,
    // );
    // executor.register(worker_pool).await?;
    // executor
    //     .run(
    //         config.checkpoints_path.clone().into(),
    //         Some(config.remote_store_url.clone()),
    //         vec![], // optional remote store access options
    //         ReaderOptions::default(),
    //         exit_receiver,
    //     )
    //     .await
}
