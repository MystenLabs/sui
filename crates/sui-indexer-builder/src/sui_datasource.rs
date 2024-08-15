// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_builder::{DataSender, Datasource};
use anyhow::Error;
use async_trait::async_trait;
use mysten_metrics::{metered_channel, spawn_monitored_task};
use std::path::PathBuf;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, Worker, WorkerPool,
};
use sui_types::base_types::TransactionDigest;
use sui_types::full_checkpoint_content::CheckpointData as SuiCheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tokio::task::JoinHandle;
use tracing::info;

pub struct SuiCheckpointDatasource {
    remote_store_url: String,
    concurrency: usize,
    checkpoint_path: PathBuf,
    metrics: DataIngestionMetrics,
}
impl SuiCheckpointDatasource {
    pub fn new(
        remote_store_url: String,
        concurrency: usize,
        checkpoint_path: PathBuf,
        metrics: DataIngestionMetrics,
    ) -> Self {
        SuiCheckpointDatasource {
            remote_store_url,
            concurrency,
            checkpoint_path,
            metrics,
        }
    }
}

#[async_trait]
impl Datasource<CheckpointTxnData> for SuiCheckpointDatasource {
    async fn start_data_retrieval(
        &self,
        starting_checkpoint: u64,
        target_checkpoint: u64,
        data_sender: DataSender<CheckpointTxnData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let (exit_sender, exit_receiver) = oneshot::channel();
        let progress_store = PerTaskInMemProgressStore {
            current_checkpoint: starting_checkpoint,
            exit_checkpoint: target_checkpoint,
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
    async fn process_checkpoint(&self, checkpoint: SuiCheckpointData) -> anyhow::Result<()> {
        info!(
            "Received checkpoint [{}] {}: {}",
            checkpoint.checkpoint_summary.epoch,
            checkpoint.checkpoint_summary.sequence_number,
            checkpoint.transactions.len(),
        );
        let checkpoint_num = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let transactions = checkpoint
            .transactions
            .into_iter()
            .map(|tx| (tx, checkpoint_num, timestamp_ms))
            .collect();
        Ok(self
            .data_sender
            .send((checkpoint_num, transactions))
            .await?)
    }
}
