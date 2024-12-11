// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::min;
use std::sync::Arc;

use anyhow::Error;
use async_trait::async_trait;
use futures::StreamExt;
use prometheus::{IntGauge, IntGaugeVec};
use tokio::task::JoinHandle;

use crate::metrics::IndexerMetricProvider;
use crate::{Task, Tasks};
use mysten_metrics::{metered_channel, spawn_monitored_task};
use tap::tap::TapFallible;

type CheckpointData<T> = (u64, Vec<T>);
pub type DataSender<T> = metered_channel::Sender<CheckpointData<T>>;

const INGESTION_BATCH_SIZE: usize = 100;
const RETRIEVED_CHECKPOINT_CHANNEL_SIZE: usize = 10000;

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
                    live_task,
                    self.storage.clone(),
                    self.data_mapper.clone(),
                );
                Some(live_task_future)
            }
            _ => None,
        };

        let backfill_tasks = ongoing_tasks.backfill_tasks_ordered_desc();
        let storage_clone = self.storage.clone();
        let data_mapper_clone = self.data_mapper.clone();
        let datasource_clone = self.datasource.clone();

        let handle = spawn_monitored_task!(async {
            // Execute tasks one by one
            for backfill_task in backfill_tasks {
                if backfill_task.start_checkpoint < backfill_task.target_checkpoint {
                    datasource_clone
                        .start_ingestion_task(
                            backfill_task,
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
            .get_largest_indexed_checkpoint(&self.name)
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
                        .register_live_task(
                            format!("{} - Live", self.name),
                            live_task_from_checkpoint,
                        )
                        .await
                        .tap_ok(|_| {
                            tracing::info!(
                                task_name = self.name.as_str(),
                                "Created live task from {}",
                                live_task_from_checkpoint,
                            );
                        })
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
                    if live_task_from_checkpoint != live_task.start_checkpoint {
                        let old_checkpoint = live_task.start_checkpoint;
                        live_task.start_checkpoint = live_task_from_checkpoint;
                        self.storage
                            .update_task(live_task)
                            .await
                            .tap_ok(|_| {
                                tracing::info!(
                                    task_name = self.name.as_str(),
                                    "Updated live task starting point from {} to {}",
                                    old_checkpoint,
                                    live_task_from_checkpoint,
                                );
                            })
                            .tap_err(|e| {
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
                        task_name = self.name.as_str(),
                        "Created backfill tasks ({}-{})",
                        from_checkpoint,
                        live_task_from_checkpoint - 1
                    );
                })
                .tap_err(|e| {
                    tracing::error!(
                        task_name = self.name.as_str(),
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

    #[cfg(any(feature = "test-utils", test))]
    pub fn test_only_name(&self) -> String {
        self.name.clone()
    }
}

#[async_trait]
pub trait Persistent<T>: IndexerProgressStore + Sync + Send + Clone {
    async fn write(&self, data: Vec<T>) -> Result<(), Error>;
}

#[async_trait]
pub trait IndexerProgressStore: Send {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64>;
    /// Attempt to save progress. Depending on the `ProgressSavingPolicy`,
    /// the progress may be cached somewhere instead of flushing to persistent storage.
    /// Returns saved checkpoint number if any. Caller can use this value as a signal
    /// to see if we have reached the target checkpoint.
    async fn save_progress(
        &mut self,
        task: &Task,
        checkpoint_numbers: &[u64],
    ) -> anyhow::Result<Option<u64>>;

    async fn get_ongoing_tasks(&self, task_prefix: &str) -> Result<Tasks, Error>;

    async fn get_largest_indexed_checkpoint(&self, prefix: &str) -> Result<Option<u64>, Error>;

    async fn register_task(
        &mut self,
        task_name: String,
        start_checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), anyhow::Error>;

    async fn register_live_task(
        &mut self,
        task_name: String,
        start_checkpoint: u64,
    ) -> Result<(), anyhow::Error>;

    async fn update_task(&mut self, task: Task) -> Result<(), Error>;
}

#[async_trait]
pub trait Datasource<T: Send>: Sync + Send {
    async fn start_ingestion_task<M, P, R>(
        &self,
        task: Task,
        mut storage: P,
        data_mapper: M,
    ) -> Result<(), Error>
    where
        M: DataMapper<T, R>,
        P: Persistent<R>,
    {
        let task_name = task.task_name.clone();
        let task_name_prefix = task.name_prefix();
        let task_type_label = task.type_str();
        let starting_checkpoint = task.start_checkpoint;
        let target_checkpoint = task.target_checkpoint;
        let ingestion_batch_size = std::env::var("INGESTION_BATCH_SIZE")
            .unwrap_or(INGESTION_BATCH_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let checkpoint_channel_size = std::env::var("RETRIEVED_CHECKPOINT_CHANNEL_SIZE")
            .unwrap_or(RETRIEVED_CHECKPOINT_CHANNEL_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        tracing::info!(
            task_name,
            ingestion_batch_size,
            checkpoint_channel_size,
            "Starting ingestion task ({}-{})",
            starting_checkpoint,
            target_checkpoint,
        );
        let (data_sender, data_rx) = metered_channel::channel(
            checkpoint_channel_size,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                // This metric works now when there is only 1 backfill task running per task name.
                // It will be unusable when there are parallel backfill tasks per task name.
                .with_label_values(&[&format!("{}-{}", task_name_prefix, task_type_label)]),
        );
        let is_live_task = task.is_live_task;
        let _live_tasks_tracker = if is_live_task {
            Some(LiveTasksTracker::new(
                self.metric_provider()
                    .get_inflight_live_tasks_metrics()
                    .clone(),
                &task_name,
            ))
        } else {
            None
        };
        let join_handle = self.start_data_retrieval(task.clone(), data_sender).await?;
        let processed_checkpoints_metrics = self
            .metric_provider()
            .get_tasks_processed_checkpoints_metric()
            .with_label_values(&[task_name_prefix, task_type_label]);
        // track remaining checkpoints per task, except for live task
        let remaining_checkpoints_metric = if !is_live_task {
            let remaining = self
                .metric_provider()
                .get_tasks_remaining_checkpoints_metric()
                .with_label_values(&[task_name_prefix]);
            remaining.set((target_checkpoint - starting_checkpoint + 1) as i64);
            Some(remaining)
        } else {
            None
        };

        let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(data_rx)
            .ready_chunks(ingestion_batch_size);
        let mut last_saved_checkpoint = None;
        loop {
            let batch_option = stream.next().await;
            if batch_option.is_none() {
                tracing::error!(task_name, "Data stream ended unexpectedly");
                break;
            }
            let batch = batch_option.unwrap();
            let mut max_height = 0;
            let mut heights = vec![];
            let mut data = vec![];
            for (height, d) in batch {
                // Filter out data with height > target_checkpoint, in case data source returns any
                if height > target_checkpoint {
                    tracing::warn!(
                        task_name,
                        height,
                        "Received data with height > target_checkpoint, skipping."
                    );
                    continue;
                }
                max_height = std::cmp::max(max_height, height);
                heights.push(height);
                data.extend(d);
            }
            tracing::debug!(
                task_name,
                max_height,
                "Ingestion task received {} blocks.",
                heights.len(),
            );
            let timer = tokio::time::Instant::now();

            if !data.is_empty() {
                let timer = tokio::time::Instant::now();
                let processed_data = data.into_iter().try_fold(vec![], |mut result, d| {
                    result.append(&mut data_mapper.map(d)?);
                    Ok::<Vec<_>, Error>(result)
                })?;
                tracing::debug!(
                    task_name,
                    max_height,
                    "Data mapper processed {} blocks in {}ms.",
                    heights.len(),
                    timer.elapsed().as_millis(),
                );
                let timer = tokio::time::Instant::now();
                // TODO: batch write data
                // TODO: we might be able to write data and progress in a single transaction.
                storage.write(processed_data).await?;
                tracing::debug!(
                    task_name,
                    max_height,
                    "Processed data ({} blocks) was wrote to storage in {}ms.",
                    heights.len(),
                    timer.elapsed().as_millis(),
                );
            }
            last_saved_checkpoint = storage.save_progress(&task, &heights).await?;
            tracing::debug!(
                task_name,
                max_height,
                last_saved_checkpoint,
                "Ingestion task processed {} blocks in {}ms",
                heights.len(),
                timer.elapsed().as_millis(),
            );
            processed_checkpoints_metrics.inc_by(heights.len() as u64);
            if let Some(m) = &remaining_checkpoints_metric {
                // Note this is only approximate as the data may come in out of order
                m.set(std::cmp::max(
                    target_checkpoint as i64 - max_height as i64,
                    0,
                ));
            }
            // If we have reached the target checkpoint, exit proactively
            if let Some(cp) = last_saved_checkpoint {
                if cp >= target_checkpoint {
                    // Task is done
                    break;
                }
            }
        }
        if is_live_task {
            // Live task should never exit, except in unit tests
            tracing::error!(task_name, "Live task exiting unexpectedly");
        } else if let Some(last_saved_checkpoint) = last_saved_checkpoint {
            if last_saved_checkpoint < target_checkpoint {
                tracing::error!(
                    task_name,
                    last_saved_checkpoint,
                    "Task exiting before reaching target checkpoint",
                );
            } else {
                tracing::info!(task_name, "Backfill task is done, exiting");
            }
        } else {
            tracing::error!(
                task_name,
                "Task exiting unexpectedly with no progress saved"
            );
        }
        join_handle.abort();
        if let Some(m) = &remaining_checkpoints_metric {
            m.set(0)
        }
        join_handle.await?.tap_err(|err| {
            tracing::error!(task_name, "Data retrieval task failed: {:?}", err);
        })
    }

    async fn start_data_retrieval(
        &self,
        task: Task,
        data_sender: DataSender<T>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error>;

    async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error>;

    fn get_genesis_height(&self) -> u64;

    fn metric_provider(&self) -> &dyn IndexerMetricProvider;
}

pub enum BackfillStrategy {
    Simple,
    Partitioned { task_size: u64 },
    Disabled,
}

pub trait DataMapper<T, R>: Sync + Send + Clone {
    fn map(&self, data: T) -> Result<Vec<R>, anyhow::Error>;
}

struct LiveTasksTracker {
    gauge: IntGauge,
}

impl LiveTasksTracker {
    pub fn new(metrics: IntGaugeVec, task_name: &str) -> Self {
        let gauge = metrics.with_label_values(&[task_name]);
        gauge.inc();
        Self { gauge }
    }
}

impl Drop for LiveTasksTracker {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}
