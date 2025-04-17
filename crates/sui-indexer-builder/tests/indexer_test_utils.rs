// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use prometheus::{IntCounterVec, IntGaugeVec};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use mysten_metrics::spawn_monitored_task;

use sui_indexer_builder::indexer_builder::{
    DataMapper, DataSender, Datasource, IndexerProgressStore, Persistent,
};
use sui_indexer_builder::metrics::IndexerMetricProvider;
use sui_indexer_builder::{Task, Tasks, LIVE_TASK_TARGET_CHECKPOINT};

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

    #[cfg(any(feature = "test-utils", test))]
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
