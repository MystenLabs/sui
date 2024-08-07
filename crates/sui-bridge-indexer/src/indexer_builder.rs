// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::min;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use async_trait::async_trait;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tokio::task::JoinHandle;
use tracing::info;

use mysten_metrics::{metered_channel, spawn_monitored_task};
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, Worker, WorkerPool,
};
use sui_types::digests::TransactionDigest;
use sui_types::full_checkpoint_content::{
    CheckpointData as SuiCheckpointData, CheckpointTransaction,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::sui_checkpoint_ingestion::{Task, Tasks};

pub type CheckpointData<T> = (u64, Vec<T>);

pub struct IndexerBuilder<D, M> {
    name: String,
    datasource: D,
    data_mapper: M,
    backfill_strategy: BackfillStrategy,
    disable_live_task: bool,
}

impl<D, M> IndexerBuilder<D, M> {
    pub fn new(name: &str, datasource: D, data_mapper: M) -> IndexerBuilder<D, M> {
        IndexerBuilder {
            name: name.into(),
            datasource,
            data_mapper,
            backfill_strategy: BackfillStrategy::Simple,
            disable_live_task: false,
        }
    }
    pub fn build<R, P>(
        self,
        start_from_checkpoint: u64,
        genesis_checkpoint: u64,
        persistent: P,
    ) -> Indexer<P, D, M>
    where
        P: Persistent<R>,
    {
        Indexer {
            name: self.name,
            storage: persistent,
            datasource: self.datasource.into(),
            backfill_strategy: self.backfill_strategy,
            disable_live_task: self.disable_live_task,
            start_from_checkpoint,
            data_mapper: self.data_mapper,
            genesis_checkpoint,
        }
    }

    pub fn with_backfill_strategy(mut self, backfill: BackfillStrategy) -> Self {
        self.backfill_strategy = backfill;
        self
    }

    pub fn disable_live_task(mut self) -> Self {
        self.disable_live_task = true;
        self
    }
}

pub struct Indexer<P, D, M> {
    name: String,
    storage: P,
    datasource: Arc<D>,
    data_mapper: M,
    backfill_strategy: BackfillStrategy,
    disable_live_task: bool,
    start_from_checkpoint: u64,
    genesis_checkpoint: u64,
}

impl<P, D, M> Indexer<P, D, M> {
    pub async fn start<T, R>(mut self) -> Result<(), Error>
    where
        D: Datasource<T> + 'static,
        M: DataMapper<T, R> + 'static,
        P: Persistent<R> + 'static,
        T: Send,
    {
        // Update tasks first
        let tasks = self.storage.tasks(&self.name)?;
        // create checkpoint workers base on backfill config and existing tasks in the db
        match tasks.live_task() {
            None => {
                // if diable_live_task is set, we should not have any live task in the db
                if !self.disable_live_task {
                    // Scenario 1: No task in database, start live task and backfill tasks
                    self.storage.register_task(
                        format!("{} - Live", self.name),
                        self.start_from_checkpoint,
                        i64::MAX,
                    )?;
                }

                // Create backfill tasks
                if self.start_from_checkpoint != self.genesis_checkpoint {
                    self.create_backfill_tasks(self.genesis_checkpoint)?
                }
            }
            Some(mut live_task) => {
                if self.disable_live_task {
                    // TODO: delete task
                    // self.storage.delete_task(live_task.task_name.clone())?;
                } else if self.start_from_checkpoint > live_task.checkpoint {
                    // Scenario 2: there are existing tasks in DB and start_from_checkpoint > current checkpoint
                    // create backfill task to finish at start_from_checkpoint
                    // update live task to start from start_from_checkpoint and finish at u64::MAX
                    self.create_backfill_tasks(live_task.checkpoint)?;
                    live_task.checkpoint = self.start_from_checkpoint;
                    self.storage.update_task(live_task)?;
                } else {
                    // Scenario 3: start_from_checkpoint < current checkpoint
                    // ignore start_from_checkpoint, resume all task as it is.
                }
            }
        }

        // get updated tasks from storage and start workers
        let updated_tasks = self.storage.tasks(&self.name)?;
        // Start latest checkpoint worker
        // Tasks are ordered in checkpoint descending order, realtime update task always come first
        // tasks won't be empty here, ok to unwrap.
        let backfill_tasks;
        let live_task_future = if self.disable_live_task {
            backfill_tasks = updated_tasks;
            None
        } else {
            let (_live_task, _backfill_tasks) = updated_tasks.split_first().unwrap();

            backfill_tasks = _backfill_tasks.to_vec();
            let live_task = _live_task;

            Some(self.datasource.start_ingestion_task(
                live_task.task_name.clone(),
                live_task.checkpoint,
                live_task.target_checkpoint,
                self.storage.clone(),
                self.data_mapper.clone(),
            ))
        };

        let backfill_tasks = backfill_tasks.to_vec();
        let storage_clone = self.storage.clone();
        let data_mapper_clone = self.data_mapper.clone();
        let datasource_clone = self.datasource.clone();

        let handle = spawn_monitored_task!(async {
            // Execute task one by one
            for backfill_task in backfill_tasks {
                datasource_clone
                    .start_ingestion_task(
                        backfill_task.task_name.clone(),
                        backfill_task.checkpoint,
                        backfill_task.target_checkpoint,
                        storage_clone.clone(),
                        data_mapper_clone.clone(),
                    )
                    .await
                    .expect("Backfill task failed");
            }
        });

        if let Some(live_task_future) = live_task_future {
            live_task_future.await?;
        }

        tokio::try_join!(handle)?;

        Ok(())
    }

    // Create backfill tasks according to backfill strategy
    fn create_backfill_tasks<R>(&mut self, mut current_cp: u64) -> Result<(), Error>
    where
        P: Persistent<R>,
    {
        match self.backfill_strategy {
            BackfillStrategy::Simple => self.storage.register_task(
                format!("{} - backfill - {}", self.name, self.start_from_checkpoint),
                current_cp + 1,
                self.start_from_checkpoint as i64,
            ),
            BackfillStrategy::Partitioned { task_size } => {
                while current_cp < self.start_from_checkpoint {
                    let target_cp = min(current_cp + task_size, self.start_from_checkpoint);
                    self.storage.register_task(
                        format!("{} - backfill - {target_cp}", self.name),
                        current_cp + 1,
                        target_cp as i64,
                    )?;
                    current_cp = target_cp;
                }
                Ok(())
            }
            BackfillStrategy::Disabled => Ok(()),
        }
    }
}

pub trait Persistent<T>: IndexerProgressStore + Sync + Send + Clone {
    fn write(&self, data: Vec<T>) -> Result<(), Error>;
}

#[async_trait]
pub trait IndexerProgressStore: Send {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64>;
    async fn save_progress(
        &mut self,
        task_name: String,
        checkpoint_number: u64,
    ) -> anyhow::Result<()>;

    fn tasks(&self, task_prefix: &str) -> Result<Vec<Task>, Error>;

    fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: i64,
    ) -> Result<(), anyhow::Error>;

    fn update_task(&mut self, task: Task) -> Result<(), Error>;
}

#[async_trait]
pub trait Datasource<T: Send>: Sync + Send {
    async fn start_ingestion_task<M, P, R>(
        &self,
        task_name: String,
        starting_checkpoint: u64,
        target_checkpoint: u64,
        mut storage: P,
        data_mapper: M,
    ) -> Result<(), Error>
    where
        M: DataMapper<T, R>,
        P: Persistent<R>,
    {
        // todo: add metrics for number of tasks
        let (data_sender, mut data_channel) = metered_channel::channel(
            1000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&[&task_name]),
        );
        let join_handle = self
            .start_data_retrieval(starting_checkpoint, target_checkpoint, data_sender)
            .await?;

        while let Some((block_number, data)) = data_channel.recv().await {
            if !data.is_empty() {
                let processed_data = data.into_iter().try_fold(vec![], |mut result, d| {
                    result.append(&mut data_mapper.map(d)?);
                    Ok::<Vec<_>, Error>(result)
                })?;
                // TODO: we might be able to write data and progress in a single transaction.
                storage.write(processed_data)?;
            }
            storage
                .save_progress(task_name.clone(), block_number)
                .await?;
        }
        join_handle.abort();
        join_handle.await?
    }

    async fn start_data_retrieval(
        &self,
        starting_checkpoint: u64,
        target_checkpoint: u64,
        data_sender: metered_channel::Sender<CheckpointData<T>>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error>;
}

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
        data_sender: metered_channel::Sender<CheckpointData<CheckpointTxnData>>,
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

pub enum BackfillStrategy {
    Simple,
    Partitioned { task_size: u64 },
    Disabled,
}

pub trait DataMapper<T, R>: Sync + Send + Clone {
    fn map(&self, data: T) -> Result<Vec<R>, anyhow::Error>;
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
