// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::progress_store::{ExecutorProgress, ProgressStore, ProgressStoreWrapper};
use crate::reader::CheckpointReader;
use crate::worker_pool::WorkerPool;
use crate::DataIngestionMetrics;
use crate::Worker;
use anyhow::Result;
use futures::Future;
use mysten_metrics::spawn_monitored_task;
use std::path::PathBuf;
use std::pin::Pin;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub const MAX_CHECKPOINTS_IN_PROGRESS: usize = 10000;

pub struct IndexerExecutor<P> {
    pools: Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    pool_senders: Vec<mpsc::Sender<CheckpointData>>,
    progress_store: ProgressStoreWrapper<P>,
    pool_progress_sender: mpsc::Sender<(String, CheckpointSequenceNumber)>,
    pool_progress_receiver: mpsc::Receiver<(String, CheckpointSequenceNumber)>,
    metrics: DataIngestionMetrics,
}

impl<P: ProgressStore> IndexerExecutor<P> {
    pub fn new(progress_store: P, number_of_jobs: usize, metrics: DataIngestionMetrics) -> Self {
        let (pool_progress_sender, pool_progress_receiver) =
            mpsc::channel(number_of_jobs * MAX_CHECKPOINTS_IN_PROGRESS);
        Self {
            pools: vec![],
            pool_senders: vec![],
            progress_store: ProgressStoreWrapper::new(progress_store),
            pool_progress_sender,
            pool_progress_receiver,
            metrics,
        }
    }

    /// Registers new worker pool in executor
    pub async fn register<W: Worker + 'static>(&mut self, pool: WorkerPool<W>) -> Result<()> {
        let checkpoint_number = self.progress_store.load(pool.task_name.clone()).await?;
        let (sender, receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        self.pools.push(Box::pin(pool.run(
            checkpoint_number,
            receiver,
            self.pool_progress_sender.clone(),
        )));
        self.pool_senders.push(sender);
        Ok(())
    }

    /// Main executor loop
    pub async fn run(
        mut self,
        path: PathBuf,
        remote_store_url: Option<String>,
        remote_store_options: Vec<(String, String)>,
        remote_read_batch_size: usize,
        mut exit_receiver: oneshot::Receiver<()>,
    ) -> Result<ExecutorProgress> {
        let mut reader_checkpoint_number = self.progress_store.min_watermark()?;
        let (checkpoint_reader, mut checkpoint_recv, gc_sender, _exit_sender) =
            CheckpointReader::initialize(
                path,
                reader_checkpoint_number,
                remote_store_url,
                remote_store_options,
                remote_read_batch_size,
            );
        spawn_monitored_task!(checkpoint_reader.run());

        for pool in std::mem::take(&mut self.pools) {
            spawn_monitored_task!(pool);
        }
        loop {
            tokio::select! {
                _ = &mut exit_receiver => break,
                Some((task_name, sequence_number)) = self.pool_progress_receiver.recv() => {
                    self.progress_store.save(task_name.clone(), sequence_number).await?;
                    let seq_number = self.progress_store.min_watermark()?;
                    if seq_number > reader_checkpoint_number {
                        gc_sender.send(seq_number).await?;
                        reader_checkpoint_number = seq_number;
                    }
                    self.metrics.data_ingestion_checkpoint.with_label_values(&[&task_name]).set(sequence_number as i64);
                }
                Some(checkpoint) = checkpoint_recv.recv() => {
                    for sender in &self.pool_senders {
                        sender.send(checkpoint.clone()).await?;
                    }
                }
            }
        }
        Ok(self.progress_store.stats())
    }
}
