// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::{max, min};
use std::sync::Arc;

use anyhow::Error;
use async_trait::async_trait;
use tokio::task::JoinHandle;

use mysten_metrics::{metered_channel, spawn_monitored_task};

use crate::{Task, Tasks};

type CheckpointData<T> = (u64, Vec<T>);
pub type DataSender<T> = metered_channel::Sender<CheckpointData<T>>;

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
        self.update_tasks().await?;
        // get updated tasks from storage and start workers
        let updated_tasks = self.storage.tasks(&self.name).await?;
        // Start latest checkpoint worker
        // Tasks are ordered in checkpoint descending order, realtime update task always come first
        // tasks won't be empty here, ok to unwrap.
        let live_task_future = match updated_tasks.live_task() {
            Some(live_task) if !self.disable_live_task => {
                let live_task_future = self.datasource.start_ingestion_task(
                    live_task.task_name.clone(),
                    live_task.checkpoint,
                    live_task.target_checkpoint,
                    self.storage.clone(),
                    self.data_mapper.clone(),
                );
                Some(live_task_future)
            }
            _ => None,
        };

        let backfill_tasks = updated_tasks.backfill_tasks();
        let storage_clone = self.storage.clone();
        let data_mapper_clone = self.data_mapper.clone();
        let datasource_clone = self.datasource.clone();

        let handle = spawn_monitored_task!(async {
            // Execute task one by one
            for backfill_task in backfill_tasks {
                if backfill_task.checkpoint < backfill_task.target_checkpoint {
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
            }
        });

        if let Some(live_task_future) = live_task_future {
            live_task_future.await?;
        }

        tokio::try_join!(handle)?;

        Ok(())
    }

    async fn update_tasks<R>(&mut self) -> Result<(), Error>
    where
        P: Persistent<R>,
    {
        let tasks = self.storage.tasks(&self.name).await?;
        let backfill_tasks = tasks.backfill_tasks();
        let latest_task = backfill_tasks.first();

        // 1, create and update live task if needed
        if !self.disable_live_task {
            let from_checkpoint = max(
                self.start_from_checkpoint,
                latest_task
                    .map(|t| t.target_checkpoint + 1)
                    .unwrap_or_default(),
            );

            match tasks.live_task() {
                None => {
                    self.storage
                        .register_task(
                            format!("{} - Live", self.name),
                            from_checkpoint,
                            i64::MAX as u64,
                        )
                        .await?;
                }
                Some(mut live_task) => {
                    if self.start_from_checkpoint > live_task.checkpoint {
                        live_task.checkpoint = self.start_from_checkpoint;
                        self.storage.update_task(live_task).await?;
                    }
                }
            }
        }

        // 2, create backfill tasks base on task config and existing tasks in the db
        match latest_task {
            None => {
                // No task in database, create backfill tasks from genesis to `start_from_checkpoint`
                if self.start_from_checkpoint != self.genesis_checkpoint {
                    self.create_backfill_tasks(
                        self.genesis_checkpoint,
                        self.start_from_checkpoint - 1,
                    )
                    .await?
                }
            }
            Some(latest_task) => {
                if latest_task.target_checkpoint + 1 < self.start_from_checkpoint {
                    self.create_backfill_tasks(
                        latest_task.target_checkpoint + 1,
                        self.start_from_checkpoint - 1,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    // Create backfill tasks according to backfill strategy
    async fn create_backfill_tasks<R>(&mut self, mut from_cp: u64, to_cp: u64) -> Result<(), Error>
    where
        P: Persistent<R>,
    {
        match self.backfill_strategy {
            BackfillStrategy::Simple => {
                self.storage
                    .register_task(
                        format!("{} - backfill - {from_cp}:{to_cp}", self.name),
                        from_cp,
                        to_cp,
                    )
                    .await
            }
            BackfillStrategy::Partitioned { task_size } => {
                while from_cp < self.start_from_checkpoint {
                    let target_cp = min(from_cp + task_size - 1, to_cp);
                    self.storage
                        .register_task(
                            format!("{} - backfill - {from_cp}:{target_cp}", self.name),
                            from_cp,
                            target_cp,
                        )
                        .await?;
                    from_cp = target_cp + 1;
                }
                Ok(())
            }
            BackfillStrategy::Disabled => Ok(()),
        }
    }
}

#[async_trait]
pub trait Persistent<T>: IndexerProgressStore + Sync + Send + Clone {
    async fn write(&self, data: Vec<T>) -> Result<(), Error>;
}

#[async_trait]
pub trait IndexerProgressStore: Send {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64>;
    async fn save_progress(
        &mut self,
        task_name: String,
        checkpoint_number: u64,
    ) -> anyhow::Result<()>;

    async fn tasks(&self, task_prefix: &str) -> Result<Vec<Task>, Error>;

    async fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), anyhow::Error>;

    async fn update_task(&mut self, task: Task) -> Result<(), Error>;
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
            if block_number > target_checkpoint {
                break;
            }
            if !data.is_empty() {
                let processed_data = data.into_iter().try_fold(vec![], |mut result, d| {
                    result.append(&mut data_mapper.map(d)?);
                    Ok::<Vec<_>, Error>(result)
                })?;
                // TODO: we might be able to write data and progress in a single transaction.
                storage.write(processed_data).await?;
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
        data_sender: DataSender<T>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error>;
}

pub enum BackfillStrategy {
    Simple,
    Partitioned { task_size: u64 },
    Disabled,
}

pub trait DataMapper<T, R>: Sync + Send + Clone {
    fn map(&self, data: T) -> Result<Vec<R>, anyhow::Error>;
}
