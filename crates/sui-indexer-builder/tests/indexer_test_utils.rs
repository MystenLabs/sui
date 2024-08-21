// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use mysten_metrics::spawn_monitored_task;

use sui_indexer_builder::indexer_builder::{
    DataMapper, DataSender, Datasource, IndexerProgressStore, Persistent,
};
use sui_indexer_builder::Task;

pub struct TestDatasource<T> {
    pub data: Vec<T>,
}

#[async_trait]
impl<T> Datasource<T> for TestDatasource<T>
where
    T: Send + Sync + Clone + 'static,
{
    async fn start_data_retrieval(
        &self,
        starting_checkpoint: u64,
        _target_checkpoint: u64,
        data_sender: DataSender<T>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let data_clone = self.data.clone();

        Ok(spawn_monitored_task!(async {
            let mut cp = starting_checkpoint;
            while cp < data_clone.len() as u64 {
                data_sender
                    .send((cp, vec![data_clone[cp as usize].clone()]))
                    .await?;
                cp += 1;
            }
            Ok(())
        }))
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
            .checkpoint)
    }

    async fn save_progress(
        &mut self,
        task_name: String,
        checkpoint_number: u64,
    ) -> anyhow::Result<()> {
        self.progress_store
            .lock()
            .await
            .get_mut(&task_name)
            .unwrap()
            .checkpoint = checkpoint_number;
        Ok(())
    }

    async fn tasks(&self, task_prefix: &str) -> Result<Vec<Task>, Error> {
        let mut tasks = self
            .progress_store
            .lock()
            .await
            .values()
            .filter(|task| task.task_name.starts_with(task_prefix))
            .cloned()
            .collect::<Vec<_>>();
        tasks.sort_by(|t1, t2| t2.checkpoint.cmp(&t1.checkpoint));
        Ok(tasks)
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
                checkpoint,
                target_checkpoint,
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
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
