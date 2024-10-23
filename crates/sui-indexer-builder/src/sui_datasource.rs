// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_builder::{DataSender, Datasource};
use crate::metrics::IndexerMetricProvider;
use crate::Task;
use anyhow::Error;
use async_trait::async_trait;
use mysten_metrics::{metered_channel, spawn_monitored_task};
use prometheus::IntGauge;
use std::path::PathBuf;
use std::sync::Arc;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, Worker, WorkerPool,
};
use sui_sdk::SuiClient;
use sui_types::full_checkpoint_content::CheckpointData as SuiCheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tokio::task::JoinHandle;

const BACKFILL_TASK_INGESTION_READER_BATCH_SIZE: usize = 300;
const LIVE_TASK_INGESTION_READER_BATCH_SIZE: usize = 10;

pub struct SuiCheckpointDatasource {
    remote_store_url: String,
    sui_client: Arc<SuiClient>,
    concurrency: usize,
    checkpoint_path: PathBuf,
    genesis_checkpoint: u64,
    ingestion_metrics: DataIngestionMetrics,
    metrics: Box<dyn IndexerMetricProvider>,
}
impl SuiCheckpointDatasource {
    pub fn new(
        remote_store_url: String,
        sui_client: Arc<SuiClient>,
        concurrency: usize,
        checkpoint_path: PathBuf,
        genesis_checkpoint: u64,
        ingestion_metrics: DataIngestionMetrics,
        metrics: Box<dyn IndexerMetricProvider>,
    ) -> Self {
        SuiCheckpointDatasource {
            remote_store_url,
            sui_client,
            concurrency,
            checkpoint_path,
            genesis_checkpoint,
            ingestion_metrics,
            metrics,
        }
    }
}

#[async_trait]
impl Datasource<CheckpointTxnData> for SuiCheckpointDatasource {
    async fn start_data_retrieval(
        &self,
        task: Task,
        data_sender: DataSender<CheckpointTxnData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let (exit_sender, exit_receiver) = oneshot::channel();
        let progress_store = PerTaskInMemProgressStore {
            current_checkpoint: task.start_checkpoint,
            exit_checkpoint: task.target_checkpoint,
            exit_sender: Some(exit_sender),
        };
        // The max concurrnecy of checkpoint to fetch at the same time for ingestion framework
        let ingestion_reader_batch_size = if task.is_live_task {
            // Live task uses smaller number to be cost effective
            LIVE_TASK_INGESTION_READER_BATCH_SIZE
        } else {
            std::env::var("BACKFILL_TASK_INGESTION_READER_BATCH_SIZE")
                .unwrap_or(BACKFILL_TASK_INGESTION_READER_BATCH_SIZE.to_string())
                .parse::<usize>()
                .unwrap()
        };
        tracing::info!(
            "Starting Sui checkpoint data retrieval with batch size {}",
            ingestion_reader_batch_size
        );
        let mut executor = IndexerExecutor::new(progress_store, 1, self.ingestion_metrics.clone());
        let progress_metric = self
            .metrics
            .get_tasks_latest_retrieved_checkpoints()
            .with_label_values(&[task.name_prefix(), task.type_str()]);
        let worker = IndexerWorker::new(data_sender, progress_metric);
        let worker_pool = WorkerPool::new(worker, task.task_name.clone(), self.concurrency);
        executor.register(worker_pool).await?;
        let checkpoint_path = self.checkpoint_path.clone();
        let remote_store_url = self.remote_store_url.clone();
        Ok(spawn_monitored_task!(async {
            executor
                .run(
                    checkpoint_path,
                    Some(remote_store_url),
                    vec![], // optional remote store access options
                    ReaderOptions {
                        batch_size: ingestion_reader_batch_size,
                        ..Default::default()
                    },
                    exit_receiver,
                )
                .await?;
            Ok(())
        }))
    }

    async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error> {
        self.sui_client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get last finalized block id: {:?}", e))
    }

    fn get_genesis_height(&self) -> u64 {
        self.genesis_checkpoint
    }

    fn metric_provider(&self) -> &dyn IndexerMetricProvider {
        self.metrics.as_ref()
    }
}

struct PerTaskInMemProgressStore {
    pub current_checkpoint: u64,
    pub exit_checkpoint: u64,
    pub exit_sender: Option<Sender<()>>,
}

#[async_trait]
impl ProgressStore for PerTaskInMemProgressStore {
    async fn load(
        &mut self,
        _task_name: String,
    ) -> Result<CheckpointSequenceNumber, anyhow::Error> {
        Ok(self.current_checkpoint)
    }

    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        if checkpoint_number >= self.exit_checkpoint {
            tracing::info!(
                task_name,
                checkpoint_number,
                exit_checkpoint = self.exit_checkpoint,
                "Task completed, sending exit signal"
            );
            // `exit_sender` may be `None` if we have already sent the exit signal.
            if let Some(sender) = self.exit_sender.take() {
                // Ignore the error if the receiver has already been dropped.
                let _ = sender.send(());
            }
        }
        self.current_checkpoint = checkpoint_number;
        Ok(())
    }
}

pub struct IndexerWorker<T> {
    data_sender: metered_channel::Sender<(u64, Vec<T>)>,
    progress_metric: IntGauge,
}

impl<T> IndexerWorker<T> {
    pub fn new(
        data_sender: metered_channel::Sender<(u64, Vec<T>)>,
        progress_metric: IntGauge,
    ) -> Self {
        Self {
            data_sender,
            progress_metric,
        }
    }
}

pub type CheckpointTxnData = (CheckpointTransaction, u64, u64);

#[async_trait]
impl Worker for IndexerWorker<CheckpointTxnData> {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &SuiCheckpointData) -> anyhow::Result<()> {
        tracing::trace!(
            "Received checkpoint [{}] {}: {}",
            checkpoint.checkpoint_summary.epoch,
            checkpoint.checkpoint_summary.sequence_number,
            checkpoint.transactions.len(),
        );
        let checkpoint_num = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let transactions = checkpoint
            .transactions
            .clone()
            .into_iter()
            .map(|tx| (tx, checkpoint_num, timestamp_ms))
            .collect();
        self.data_sender
            .send((checkpoint_num, transactions))
            .await?;
        self.progress_metric.set(checkpoint_num as i64);
        Ok(())
    }
}
