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

    #[allow(dead_code)]
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

    #[cfg(test)]
    pub async fn test_only_update_tasks<R, T>(&mut self) -> Result<(), Error>
    where
        P: Persistent<R>,
        D: Datasource<T>,
        T: Send,
    {
        self.update_tasks().await
    }

    #[cfg(test)]
    pub fn test_only_storage<R>(&self) -> &P
    where
        P: Persistent<R>,
    {
        &self.storage
    }

    #[cfg(test)]
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
            if let Some(cp) = last_saved_checkpoint
                && cp >= target_checkpoint
            {
                // Task is done
                break;
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::{Error, anyhow};
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use tokio::task::JoinHandle;

    use mysten_metrics::spawn_monitored_task;

    use crate::indexer_builder::{
        DataMapper, DataSender, Datasource, IndexerProgressStore, Persistent,
    };
    use crate::metrics::IndexerMetricProvider;
    use crate::{LIVE_TASK_TARGET_CHECKPOINT, Task, Tasks};

    use crate::indexer_builder::{BackfillStrategy, IndexerBuilder};
    use prometheus::{
        IntCounterVec, IntGaugeVec, Registry, register_int_counter_vec_with_registry,
        register_int_gauge_vec_with_registry,
    };

    pub struct TestDatasource<T> {
        pub data: Vec<T>,
        pub live_task_starting_checkpoint: u64,
        pub genesis_checkpoint: u64,
        pub gauge_metric: IntGaugeVec,
        pub counter_metric: IntCounterVec,
        pub inflight_live_tasks: IntGaugeVec,
    }

    #[async_trait]
    impl<T> Datasource<T> for TestDatasource<T>
    where
        T: Send + Sync + Clone + 'static,
    {
        async fn start_data_retrieval(
            &self,
            task: Task,
            data_sender: DataSender<T>,
        ) -> Result<JoinHandle<Result<(), Error>>, Error> {
            let data_clone = self.data.clone();

            Ok(spawn_monitored_task!(async {
                let mut cp = task.start_checkpoint;
                while cp < data_clone.len() as u64 {
                    data_sender
                        .send((cp, vec![data_clone[cp as usize].clone()]))
                        .await?;
                    cp += 1;
                }
                Ok(())
            }))
        }

        async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error> {
            Ok(self.live_task_starting_checkpoint)
        }

        fn get_genesis_height(&self) -> u64 {
            self.genesis_checkpoint
        }

        fn metric_provider(&self) -> &dyn IndexerMetricProvider {
            self
        }
    }

    impl<T: Send + Sync> IndexerMetricProvider for TestDatasource<T> {
        fn get_tasks_latest_retrieved_checkpoints(&self) -> &IntGaugeVec {
            // This is dummy
            &self.gauge_metric
        }

        fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec {
            // This is dummy
            &self.gauge_metric
        }

        fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec {
            // This is dummy
            &self.counter_metric
        }

        fn get_inflight_live_tasks_metrics(&self) -> &IntGaugeVec {
            // This is dummy
            &self.inflight_live_tasks
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct InMemoryPersistent<T> {
        pub progress_store: Arc<Mutex<HashMap<String, Task>>>,
        pub data: Arc<Mutex<Vec<T>>>,
    }

    impl<T> InMemoryPersistent<T> {
        pub fn new() -> Self {
            InMemoryPersistent {
                progress_store: Default::default(),
                data: Arc::new(Mutex::new(vec![])),
            }
        }

        #[cfg(test)]
        pub async fn get_all_tasks(&self, task_prefix: &str) -> Result<Vec<Task>, Error> {
            let mut tasks = self
                .progress_store
                .lock()
                .await
                .values()
                .filter(|task| task.task_name.starts_with(task_prefix))
                .cloned()
                .collect::<Vec<_>>();
            tasks.sort_by(|t1, t2| t2.start_checkpoint.cmp(&t1.start_checkpoint));
            Ok(tasks)
        }

        async fn get_largest_backfill_task_target_checkpoint(
            &self,
            task_prefix: &str,
        ) -> Result<Option<u64>, Error> {
            Ok(self
                .progress_store
                .lock()
                .await
                .values()
                .filter(|task| task.task_name.starts_with(task_prefix))
                .filter(|task| task.target_checkpoint.ne(&(i64::MAX as u64)))
                .max_by(|t1, t2| t1.target_checkpoint.cmp(&t2.target_checkpoint))
                .map(|t| t.target_checkpoint))
        }
    }

    #[async_trait]
    impl<T: Send + Sync> IndexerProgressStore for InMemoryPersistent<T> {
        async fn load_progress(&self, task_name: String) -> anyhow::Result<u64> {
            Ok(self
                .progress_store
                .lock()
                .await
                .get(&task_name)
                .unwrap()
                .start_checkpoint)
        }

        async fn save_progress(
            &mut self,
            task: &Task,
            checkpoint_numbers: &[u64],
        ) -> anyhow::Result<Option<u64>> {
            let checkpoint_number = *checkpoint_numbers.last().unwrap();
            self.progress_store
                .lock()
                .await
                .get_mut(&task.task_name)
                .unwrap()
                .start_checkpoint = checkpoint_number;
            Ok(Some(checkpoint_number))
        }

        async fn get_ongoing_tasks(&self, task_prefix: &str) -> Result<Tasks, Error> {
            let tasks = self
                .progress_store
                .lock()
                .await
                .values()
                .filter(|task| task.task_name.starts_with(task_prefix))
                .filter(|task| task.start_checkpoint.lt(&task.target_checkpoint))
                .cloned()
                .collect::<Vec<_>>();
            Tasks::new(tasks)
        }

        async fn get_largest_indexed_checkpoint(
            &self,
            task_prefix: &str,
        ) -> Result<Option<u64>, Error> {
            let checkpoint = self
                .progress_store
                .lock()
                .await
                .values()
                .filter(|task| task.task_name.starts_with(task_prefix))
                .filter(|task| task.target_checkpoint.eq(&(i64::MAX as u64)))
                .last()
                .map(|t| t.start_checkpoint);

            if checkpoint.is_some() {
                Ok(checkpoint)
            } else {
                self.get_largest_backfill_task_target_checkpoint(task_prefix)
                    .await
            }
        }

        async fn register_task(
            &mut self,
            task_name: String,
            checkpoint: u64,
            target_checkpoint: u64,
        ) -> Result<(), Error> {
            let existing = self.progress_store.lock().await.insert(
                task_name.clone(),
                Task {
                    task_name: task_name.clone(),
                    start_checkpoint: checkpoint,
                    target_checkpoint,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                    is_live_task: false,
                },
            );
            if existing.is_some() {
                return Err(anyhow!("Task {task_name} already exists"));
            }
            Ok(())
        }

        async fn register_live_task(
            &mut self,
            task_name: String,
            checkpoint: u64,
        ) -> Result<(), Error> {
            let existing = self.progress_store.lock().await.insert(
                task_name.clone(),
                Task {
                    task_name: task_name.clone(),
                    start_checkpoint: checkpoint,
                    target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                    is_live_task: true,
                },
            );
            if existing.is_some() {
                return Err(anyhow!("Task {task_name} already exists"));
            }
            Ok(())
        }

        async fn update_task(&mut self, task: Task) -> Result<(), Error> {
            self.progress_store
                .lock()
                .await
                .insert(task.task_name.clone(), task);
            Ok(())
        }
    }

    #[async_trait]
    impl<T: Clone + Send + Sync> Persistent<T> for InMemoryPersistent<T> {
        async fn write(&self, data: Vec<T>) -> Result<(), Error> {
            self.data.lock().await.append(&mut data.clone());
            Ok(())
        }
    }

    #[derive(Clone)]
    pub struct NoopDataMapper;

    impl<T> DataMapper<T, T> for NoopDataMapper {
        fn map(&self, data: T) -> Result<Vec<T>, Error> {
            Ok(vec![data])
        }
    }

    #[tokio::test]
    async fn indexer_simple_backfill_task_test() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=10u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 5,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(5, i64::MAX as u64), (0, 4)]);
        indexer.start().await.unwrap();

        // it should have 2 task created for the indexer - a live task and a backfill task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        println!("{:?}", tasks);
        assert_ranges(&tasks, vec![(10, i64::MAX as u64), (4, 4)]);
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!(data, recorded_data);
    }

    #[tokio::test]
    async fn indexer_partitioned_backfill_task_test() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 35,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(
            &tasks,
            vec![(35, i64::MAX as u64), (30, 34), (20, 29), (10, 19), (0, 9)],
        );
        indexer.start().await.unwrap();

        // it should have 5 task created for the indexer - a live task and 4 backfill task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(
            &tasks,
            vec![(50, i64::MAX as u64), (34, 34), (29, 29), (19, 19), (9, 9)],
        );
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!(data, recorded_data);
    }

    #[tokio::test]
    async fn indexer_partitioned_task_with_data_already_in_db_test1() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 31,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.data.lock().await.append(&mut (0..=30).collect());
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 1".to_string(),
            Task {
                task_name: "test_indexer - backfill - 1".to_string(),
                start_checkpoint: 30,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(31, i64::MAX as u64), (30, 30)]);
        indexer.start().await.unwrap();

        // it should have 2 task created for the indexer, one existing task and one live task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30)]);
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!(data, recorded_data);
    }

    #[tokio::test]
    async fn indexer_partitioned_task_with_data_already_in_db_test2() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 35,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.data.lock().await.append(&mut (0..=30).collect());
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 1".to_string(),
            Task {
                task_name: "test_indexer - backfill - 1".to_string(),
                start_checkpoint: 30,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(35, i64::MAX as u64), (31, 34), (30, 30)]);
        indexer.start().await.unwrap();

        // it should have 3 tasks created for the indexer, existing task, a backfill task from cp 31 to cp 34, and a live task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64), (34, 34), (30, 30)]);
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!(data, recorded_data);
    }

    // `live_task_from_checkpoint` is smaller than the largest checkpoint in DB.
    // The live task should start from `live_task_from_checkpoint`.
    #[tokio::test]
    async fn indexer_partitioned_task_with_data_already_in_db_test3() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 28,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 20:30".to_string(),
            Task {
                task_name: "test_indexer - backfill - 20:30".to_string(),
                start_checkpoint: 30,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 10:19".to_string(),
            Task {
                task_name: "test_indexer - backfill - 10:19".to_string(),
                start_checkpoint: 10,
                target_checkpoint: 19,
                timestamp: 0,
                is_live_task: false,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(30, 30), (28, i64::MAX as u64), (10, 19)]);
        indexer.start().await.unwrap();

        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30), (19, 19)]);
    }

    // `live_task_from_checkpoint` is larger than the largest checkpoint in DB.
    // The live task should start from `live_task_from_checkpoint`.
    #[tokio::test]
    async fn indexer_partitioned_task_with_data_already_in_db_test4() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 35,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 20:30".to_string(),
            Task {
                task_name: "test_indexer - backfill - 20:30".to_string(),
                start_checkpoint: 30,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 10:19".to_string(),
            Task {
                task_name: "test_indexer - backfill - 10:19".to_string(),
                start_checkpoint: 10,
                target_checkpoint: 19,
                timestamp: 0,
                is_live_task: false,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(
            &tasks,
            vec![(35, i64::MAX as u64), (31, 34), (30, 30), (10, 19)],
        );
        indexer.start().await.unwrap();

        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(
            &tasks,
            vec![(50, i64::MAX as u64), (34, 34), (30, 30), (19, 19)],
        );
    }

    #[tokio::test]
    async fn indexer_with_existing_live_task1() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 35,
            genesis_checkpoint: 10,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - Live".to_string(),
            Task {
                task_name: "test_indexer - Live".to_string(),
                start_checkpoint: 30,
                target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
                timestamp: 0,
                is_live_task: true,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Simple)
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(35, i64::MAX as u64), (31, 34)]);
        indexer.start().await.unwrap();

        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64), (34, 34)]);
    }

    #[tokio::test]
    async fn indexer_with_existing_live_task2() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 25,
            genesis_checkpoint: 10,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - Live".to_string(),
            Task {
                task_name: "test_indexer - Live".to_string(),
                start_checkpoint: 30,
                target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
                timestamp: 10,
                is_live_task: true,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Simple)
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        println!("{tasks:?}");
        assert_ranges(&tasks, vec![(25, i64::MAX as u64)]);
        indexer.start().await.unwrap();

        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64)]);
    }

    fn assert_ranges(desc_ordered_tasks: &[Task], ranges: Vec<(u64, u64)>) {
        assert!(desc_ordered_tasks.len() == ranges.len());
        let mut iter = desc_ordered_tasks.iter();
        for (start, end) in ranges {
            let task = iter.next().unwrap();
            assert_eq!(start, task.start_checkpoint);
            assert_eq!(end, task.target_checkpoint);
        }
    }

    #[tokio::test]
    async fn resume_test() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=50u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 31,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 30".to_string(),
            Task {
                task_name: "test_indexer - backfill - 30".to_string(),
                start_checkpoint: 10,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Simple)
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(31, i64::MAX as u64), (10, 30)]);
        indexer.start().await.unwrap();

        // it should have 2 task created for the indexer, one existing task and one live task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30)]);
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!((10..=50u64).collect::<Vec<_>>(), recorded_data);
    }

    #[tokio::test]
    async fn resume_with_live_test() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let data = (0..=70u64).collect::<Vec<_>>();
        let datasource = TestDatasource {
            data: data.clone(),
            live_task_starting_checkpoint: 60,
            genesis_checkpoint: 0,
            gauge_metric: new_gauge_vec(&registry, "foo"),
            counter_metric: new_counter_vec(&registry),
            inflight_live_tasks: new_gauge_vec(&registry, "bar"),
        };
        let persistent = InMemoryPersistent::new();
        persistent.progress_store.lock().await.insert(
            "test_indexer - backfill - 30".to_string(),
            Task {
                task_name: "test_indexer - backfill - 30".to_string(),
                start_checkpoint: 10,
                target_checkpoint: 30,
                timestamp: 0,
                is_live_task: false,
            },
        );
        persistent.progress_store.lock().await.insert(
            "test_indexer - Live".to_string(),
            Task {
                task_name: "test_indexer - Live".to_string(),
                start_checkpoint: 50,
                target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
                timestamp: 10,
                is_live_task: true,
            },
        );
        // the live task have indexed cp 31 to 50 before shutdown
        persistent
            .data
            .lock()
            .await
            .append(&mut (31..=50).collect());
        let mut indexer = IndexerBuilder::new(
            "test_indexer",
            datasource,
            NoopDataMapper,
            persistent.clone(),
        )
        .with_backfill_strategy(BackfillStrategy::Simple)
        .build();
        indexer.test_only_update_tasks().await.unwrap();
        let tasks = indexer
            .test_only_storage()
            .get_all_tasks("test_indexer")
            .await
            .unwrap();
        assert_ranges(&tasks, vec![(60, i64::MAX as u64), (51, 59), (10, 30)]);
        indexer.start().await.unwrap();

        // it should have 2 task created for the indexer, one existing task and one live task
        let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
        assert_ranges(&tasks, vec![(70, i64::MAX as u64), (59, 59), (30, 30)]);
        // the data recorded in storage should be the same as the datasource
        let mut recorded_data = persistent.data.lock().await.clone();
        recorded_data.sort();
        assert_eq!((10..=70u64).collect::<Vec<_>>(), recorded_data);
    }

    fn new_gauge_vec(registry: &Registry, name: &str) -> IntGaugeVec {
        register_int_gauge_vec_with_registry!(name, "whatever", &["whatever"], registry,).unwrap()
    }

    fn new_counter_vec(registry: &Registry) -> IntCounterVec {
        register_int_counter_vec_with_registry!(
            "whatever_counter",
            "whatever",
            &["whatever1", "whatever2"],
            registry,
        )
        .unwrap()
    }
}
