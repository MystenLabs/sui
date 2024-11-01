// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use async_trait::async_trait;
use mysten_metrics::{metered_channel, spawn_monitored_task};
use prometheus::IntCounterVec;
use prometheus::IntGaugeVec;
use std::path::PathBuf;
use std::sync::Arc;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, Worker, WorkerPool,
};
use sui_indexer_builder::indexer_builder::{DataSender, Datasource};
use sui_indexer_builder::Task;
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::full_checkpoint_content::CheckpointData as SuiCheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tokio::task::JoinHandle;

use crate::metrics::DeepBookIndexerMetrics;

pub struct SuiCheckpointDatasource {
    remote_store_url: String,
    sui_client: Arc<SuiClient>,
    concurrency: usize,
    checkpoint_path: PathBuf,
    genesis_checkpoint: u64,
    metrics: DataIngestionMetrics,
    indexer_metrics: DeepBookIndexerMetrics,
}
impl SuiCheckpointDatasource {
    pub fn new(
        remote_store_url: String,
        sui_client: Arc<SuiClient>,
        concurrency: usize,
        checkpoint_path: PathBuf,
        genesis_checkpoint: u64,
        metrics: DataIngestionMetrics,
        indexer_metrics: DeepBookIndexerMetrics,
    ) -> Self {
        SuiCheckpointDatasource {
            remote_store_url,
            sui_client,
            concurrency,
            checkpoint_path,
            metrics,
            indexer_metrics,
            genesis_checkpoint,
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
        let mut executor = IndexerExecutor::new(progress_store, 1, self.metrics.clone());
        let worker = IndexerWorker::new(data_sender);
        let worker_pool = WorkerPool::new(
            worker,
            TransactionDigest::random().to_string(),
            self.concurrency,
        );
        executor.register(worker_pool).await?;
        let checkpoint_path = self.checkpoint_path.clone();
        let remote_store_url = self.remote_store_url.clone();
        Ok(spawn_monitored_task!(async {
            executor
                .run(
                    checkpoint_path,
                    Some(remote_store_url),
                    vec![], // optional remote store access options
                    ReaderOptions::default(),
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

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec {
        &self.indexer_metrics.backfill_tasks_remaining_checkpoints
    }

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec {
        &self.indexer_metrics.tasks_processed_checkpoints
    }

    fn get_inflight_live_tasks_metrics(&self) -> &IntGaugeVec {
        &self.indexer_metrics.inflight_live_tasks
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
        _task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        if checkpoint_number >= self.exit_checkpoint {
            if let Some(sender) = self.exit_sender.take() {
                let _ = sender.send(());
            }
        }
        self.current_checkpoint = checkpoint_number;
        Ok(())
    }
}

pub struct IndexerWorker<T> {
    data_sender: metered_channel::Sender<(u64, Vec<T>)>,
}

impl<T> IndexerWorker<T> {
    pub fn new(data_sender: metered_channel::Sender<(u64, Vec<T>)>) -> Self {
        Self { data_sender }
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
        Ok(self
            .data_sender
            .send((checkpoint_num, transactions))
            .await?)
    }
}
