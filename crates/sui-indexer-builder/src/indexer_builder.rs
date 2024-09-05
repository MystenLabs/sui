// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::min;
use std::sync::Arc;

use anyhow::Error;
use async_trait::async_trait;
use tokio::task::JoinHandle;

use crate::{Task, Tasks};
use mysten_metrics::{metered_channel, spawn_monitored_task};
use tap::tap::TapFallible;

type CheckpointData<T> = (u64, Vec<T>);
pub type DataSender<T> = metered_channel::Sender<CheckpointData<T>>;

pub struct IndexerBuilder<D, M, P> {
    name: String,
    datasource: D,
    data_mapper: M,
    persistent: P,
    backfill_strategy: BackfillStrategy,
    disable_live_task: bool,
}

impl<D, M, P> IndexerBuilder<D, M, P> {
    pub fn new<R>(
        name: &str,
        datasource: D,
        data_mapper: M,
        persistent: P,
    ) -> IndexerBuilder<D, M, P>
    where
        P: Persistent<R>,
    {
        IndexerBuilder {
            name: name.into(),
            datasource,
            data_mapper,
            backfill_strategy: BackfillStrategy::Simple,
            disable_live_task: false,
            persistent,
        }
    }
    pub fn build(self) -> Indexer<P, D, M> {
        Indexer {
            name: self.name,
            storage: self.persistent,
            datasource: self.datasource.into(),
            backfill_strategy: self.backfill_strategy,
            disable_live_task: self.disable_live_task,
            data_mapper: self.data_mapper,
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
}

impl<P, D, M> Indexer<P, D, M> {
    pub async fn start<T, R>(mut self) -> Result<(), Error>
    where
        D: Datasource<T> + 'static,
        M: DataMapper<T, R> + 'static,
        P: Persistent<R> + 'static,
        T: Send,
    {
        let task_name = self.name.clone();
        // Update tasks first
        self.update_tasks()
            .await
            .tap_err(|e| {
                tracing::error!(task_name, "Failed to update tasks: {:?}", e);
            })
            .tap_ok(|_| {
                tracing::info!(task_name, "Tasks updated.");
            })?;

        // get ongoing tasks from storage
        let ongoing_tasks = self
            .storage
            .get_ongoing_tasks(&self.name)
            .await
            .tap_err(|e| {
                tracing::error!(task_name, "Failed to get updated tasks: {:?}", e);
            })
            .tap_ok(|tasks| {
                tracing::info!(task_name, "Got updated tasks: {:?}", tasks);
            })?;

        // Start latest checkpoint worker
        // Tasks are ordered in checkpoint descending order, realtime update task always come first
        // tasks won't be empty here, ok to unwrap.
        let live_task_future = match ongoing_tasks.live_task() {
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

        let backfill_tasks = ongoing_tasks.backfill_tasks();
        let storage_clone = self.storage.clone();
        let data_mapper_clone = self.data_mapper.clone();
        let datasource_clone = self.datasource.clone();

        let handle = spawn_monitored_task!(async {
            // Execute tasks one by one
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

    async fn update_tasks<T, R>(&mut self) -> Result<(), Error>
    where
        P: Persistent<R>,
        D: Datasource<T>,
        T: Send,
    {
        let ongoing_tasks = self.storage.get_ongoing_tasks(&self.name).await?;
        let largest_checkpoint = self
            .storage
            .get_largest_backfill_task_target_checkpoint(&self.name)
            .await?;
        let live_task_from_checkpoint = self.datasource.get_live_task_starting_checkpoint().await?;

        // Create and update live task if needed
        // for live task, we always start from `live_task_from_checkpoint`.
        // What if there are older tasks with larger height? It's very
        // unlikely, and even if it happens, we just reprocess the range.
        // This simplifies the logic of determining task boundaries.
        if !self.disable_live_task {
            match ongoing_tasks.live_task() {
                None => {
                    self.storage
                        .register_task(
                            format!("{} - Live", self.name),
                            live_task_from_checkpoint,
                            i64::MAX as u64,
                        )
                        .await
                        .tap_err(|e| {
                            tracing::error!(
                                "Failed to register live task ({}-MAX): {:?}",
                                live_task_from_checkpoint,
                                e
                            );
                        })?;
                }
                Some(mut live_task) => {
                    // We still check this because in the case of slow
                    // block generation (e.g. Ethereum), it's possible we will
                    // stay on the same block for a bit.
                    if live_task_from_checkpoint != live_task.checkpoint {
                        live_task.checkpoint = live_task_from_checkpoint;
                        self.storage.update_task(live_task).await.tap_err(|e| {
                            tracing::error!(
                                "Failed to update live task to ({}-MAX): {:?}",
                                live_task_from_checkpoint,
                                e
                            );
                        })?;
                    }
                }
            }
        }

        // 2, if there is a gap between `largest_checkpoint` and `live_task_from_checkpoint`,
        // create backfill task [largest_checkpoint + 1, live_task_from_checkpoint - 1]

        // TODO: when there is a hole, we create one task for the hole, but ideally we should
        // honor the partition size and create as needed.
        let from_checkpoint = largest_checkpoint
            .map(|cp| cp + 1)
            .unwrap_or(self.datasource.get_genesis_height());
        if from_checkpoint < live_task_from_checkpoint {
            self.create_backfill_tasks(from_checkpoint, live_task_from_checkpoint - 1)
                .await
                .tap_ok(|_| {
                    tracing::info!(
                        "Created backfill tasks ({}-{})",
                        from_checkpoint,
                        live_task_from_checkpoint - 1
                    );
                })
                .tap_err(|e| {
                    tracing::error!(
                        "Failed to create backfill tasks ({}-{}): {:?}",
                        from_checkpoint,
                        live_task_from_checkpoint - 1,
                        e
                    );
                })?;
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
                // TODO: register all tasks in one DB write
                while from_cp < to_cp {
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

    #[cfg(any(feature = "test-utils", test))]
    pub async fn test_only_update_tasks<R, T>(&mut self) -> Result<(), Error>
    where
        P: Persistent<R>,
        D: Datasource<T>,
        T: Send,
    {
        self.update_tasks().await
    }

    #[cfg(any(feature = "test-utils", test))]
    pub fn test_only_storage<R>(&self) -> &P
    where
        P: Persistent<R>,
    {
        &self.storage
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

    async fn get_ongoing_tasks(&self, task_prefix: &str) -> Result<Vec<Task>, Error>;

    async fn get_largest_backfill_task_target_checkpoint(
        &self,
        task_prefix: &str,
    ) -> Result<Option<u64>, Error>;

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
        tracing::info!(
            task_name,
            "Starting ingestion task ({}-{})",
            starting_checkpoint,
            target_checkpoint
        );
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

    async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error>;

    fn get_genesis_height(&self) -> u64;
}

pub enum BackfillStrategy {
    Simple,
    Partitioned { task_size: u64 },
    Disabled,
}

pub trait DataMapper<T, R>: Sync + Send + Clone {
    fn map(&self, data: T) -> Result<Vec<R>, anyhow::Error>;
}
