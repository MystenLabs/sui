// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::*;
use mysten_metrics::spawn_logged_monitored_task;
use mysten_metrics::{spawn_monitored_task, start_prometheus_server};
use prometheus::Registry;
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge_indexer::eth_worker::EthBridgeWorker;
use sui_bridge_indexer::postgres_manager::{
    get_connection_pool, read_sui_progress_store, PgProgressStore,
};
use sui_bridge_indexer::sui_transaction_handler::handle_sui_transactions_loop;
use sui_bridge_indexer::sui_transaction_queries::start_sui_tx_polling_task;
use sui_bridge_indexer::sui_worker::SuiBridgeWorker;
use sui_bridge_indexer::{config, config::load_config, metrics::BridgeIndexerMetrics};
use sui_data_ingestion_core::{DataIngestionMetrics, IndexerExecutor, ReaderOptions, WorkerPool};
use sui_sdk::SuiClientBuilder;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::task::JoinHandle;

use sui_types::digests::TransactionDigest;
use mysten_metrics::metered_channel::channel;
use sui_bridge_indexer::config::IndexerConfig;
use sui_config::Config;
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
    let config = IndexerConfig::load(&config_path)?;
    let config_clone = config.clone();

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
        EthClient::<ethers::providers::Http>::new(
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
        start_processing_sui_checkpoints(
            &config_clone,
            db_url,
            indexer_meterics,
            ingestion_metrics,
        )
        .await?;
    }
    // We are not waiting for the sui tasks to finish here, which is ok.
    futures::future::join_all(handles).await;

    Ok(())
}

async fn start_processing_sui_checkpoints(
    config: &IndexerConfig,
    db_url: String,
    indexer_meterics: BridgeIndexerMetrics,
    ingestion_metrics: DataIngestionMetrics,
) -> Result<(), anyhow::Error> {
    // metrics init
    let pg_pool = get_connection_pool(db_url.clone());
    let progress_store = PgProgressStore::new(pg_pool, config.bridge_genesis_checkpoint);

    // Update tasks first
    let tasks = progress_store.tasks()?;
    // checkpoint workers
    match tasks.latest_checkpoint_task() {
        None => {
            // No task in database, start latest checkpoint task and backfill tasks
            // if resume_from_checkpoint, use it for the latest task, if not set, use bridge_genesis_checkpoint
            let start_from_cp = config
                .resume_from_checkpoint
                .unwrap_or_else(|| config.bridge_genesis_checkpoint);
            progress_store.register_task(new_task_name(), start_from_cp, i64::MAX)?;

            // Create backfill tasks
            if start_from_cp != config.bridge_genesis_checkpoint {
                let mut current_cp = config.bridge_genesis_checkpoint;
                while current_cp < start_from_cp {
                    let target_cp = min(current_cp + config.back_fill_lot_size, start_from_cp);
                    progress_store.register_task(new_task_name(), current_cp, target_cp as i64)?;
                    current_cp = target_cp;
                }
            }
        }
        Some(mut task) => {
            match config.resume_from_checkpoint {
                Some(cp) if task.checkpoint < cp => {
                    // Scenario 1: resume_from_checkpoint is set, and it's > current checkpoint
                    // create new task from resume_from_checkpoint to u64::MAX
                    // Update old task to finish at resume_from_checkpoint
                    let mut target_cp = cp;
                    while target_cp - task.checkpoint > config.back_fill_lot_size {
                        progress_store.register_task(
                            new_task_name(),
                            target_cp - config.back_fill_lot_size,
                            target_cp as i64,
                        )?;
                        target_cp = target_cp - config.back_fill_lot_size;
                    }
                    task.target_checkpoint = target_cp;
                    progress_store.update_task(task)?;
                    progress_store.register_task(new_task_name(), cp, i64::MAX)?;
                }
                _ => {
                    // Scenario 2: resume_from_checkpoint is set, but it's < current checkpoint or not set
                    // ignore resume_from_checkpoint, resume all task as it is.
                }
            }
        }
    }

    // get updated tasks and start workers
    let updated_tasks = progress_store.tasks()?;
    // Start latest checkpoint worker
    // Tasks are ordered in checkpoint descending order, realtime update task always come first
    // task won't be empty here, ok to unwrap.
    let (realtime_task, backfill_tasks) = updated_tasks.split_first().unwrap();
    let ingestion_metrics_clone = ingestion_metrics.clone();
    let indexer_meterics_clone = indexer_meterics.clone();
    let db_url_clone = db_url.clone();
    let progress_store_clone = progress_store.clone();
    let config_clone = config.clone();
    let backfill_tasks = backfill_tasks.to_vec();
    let handle = spawn_monitored_task!(async {
        for backfill_task in backfill_tasks {
            start_executor(
                progress_store_clone.clone(),
                ingestion_metrics_clone.clone(),
                indexer_meterics_clone.clone(),
                &config_clone,
                db_url_clone.clone(),
                &backfill_task,
            )
            .await
            .expect("Backfill task failed");
        }
    });
    start_executor(
        progress_store,
        ingestion_metrics,
        indexer_meterics,
        config,
        db_url,
        realtime_task,
    )
    .await?;
    tokio::try_join!(handle)?;
    Ok(())
}

async fn start_executor(
    progress_store: PgProgressStore,
    ingestion_metrics: DataIngestionMetrics,
    indexer_meterics: BridgeIndexerMetrics,
    config: &Config,
    db_url: String,
    task: &Task,
) -> Result<(), anyhow::Error> {
    let (_exit_sender, exit_receiver) = oneshot::channel();
    let mut executor = IndexerExecutor::new_with_upper_limit(
        progress_store,
        1, /* workflow types */
        ingestion_metrics,
        task.target_checkpoint,
    );

    let indexer_metrics_cloned = indexer_meterics.clone();

    let worker_pool = WorkerPool::new(
        SuiBridgeWorker::new(vec![], db_url, indexer_metrics_cloned),
        task.task_name.clone(),
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
        .await?;
    Ok(())
}

fn new_task_name() -> String {
    format!("bridge worker - {}", TransactionDigest::random())
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
