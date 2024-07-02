// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::IndexerConfig;
use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::sui_worker::SuiBridgeWorker;
use crate::{models, schema};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::{
    delete, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper,
};
use mysten_metrics::spawn_monitored_task;
use std::cmp::min;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, WorkerPool,
};
use sui_types::base_types::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;

pub struct SuiCheckpointSyncer {
    pool: PgPool,
    bridge_genesis_checkpoint: u64,
}

impl SuiCheckpointSyncer {
    pub fn new(pool: PgPool, bridge_genesis_checkpoint: u64) -> Self {
        // read all task from db
        SuiCheckpointSyncer {
            pool: pool.clone(),
            bridge_genesis_checkpoint,
        }
    }
    pub async fn start(
        self,
        config: &IndexerConfig,
        indexer_meterics: BridgeIndexerMetrics,
        ingestion_metrics: DataIngestionMetrics,
    ) -> anyhow::Result<(), anyhow::Error> {
        // Update tasks first
        let tasks = self.tasks()?;
        // checkpoint workers
        match tasks.latest_checkpoint_task() {
            None => {
                // No task in database, start latest checkpoint task and backfill tasks
                // if resume_from_checkpoint, use it for the latest task, if not set, use bridge_genesis_checkpoint
                let start_from_cp = config
                    .resume_from_checkpoint
                    .unwrap_or(self.bridge_genesis_checkpoint);
                self.register_task(new_task_name(), start_from_cp, i64::MAX)?;

                // Create backfill tasks
                if start_from_cp != config.bridge_genesis_checkpoint {
                    let mut current_cp = self.bridge_genesis_checkpoint;
                    while current_cp < start_from_cp {
                        let target_cp = min(current_cp + config.back_fill_lot_size, start_from_cp);
                        self.register_task(new_task_name(), current_cp, target_cp as i64)?;
                        current_cp = target_cp;
                    }
                }
            }
            Some(mut task) => {
                match config.resume_from_checkpoint {
                    Some(cp) if task.checkpoint < cp => {
                        // Scenario 1: resume_from_checkpoint is set, and it's > current checkpoint
                        // create new task from resume_from_checkpoint to u64::MAX
                        // Update old task to finish at resume_from_checkpoint
                        let mut target_cp = cp;
                        while target_cp - task.checkpoint > config.back_fill_lot_size {
                            self.register_task(
                                new_task_name(),
                                target_cp - config.back_fill_lot_size,
                                target_cp as i64,
                            )?;
                            target_cp -= config.back_fill_lot_size;
                        }
                        task.target_checkpoint = target_cp;
                        self.update_task(task)?;
                        self.register_task(new_task_name(), cp, i64::MAX)?;
                    }
                    _ => {
                        // Scenario 2: resume_from_checkpoint is set, but it's < current checkpoint or not set
                        // ignore resume_from_checkpoint, resume all task as it is.
                    }
                }
            }
        }

        // get updated tasks and start workers
        let updated_tasks = self.tasks()?;
        // Start latest checkpoint worker
        // Tasks are ordered in checkpoint descending order, realtime update task always come first
        // tasks won't be empty here, ok to unwrap.
        let (realtime_task, backfill_tasks) = updated_tasks.split_first().unwrap();
        let realtime_task_future = Self::start_executor(
            self.pool.clone(),
            self.bridge_genesis_checkpoint,
            ingestion_metrics.clone(),
            indexer_meterics.clone(),
            config,
            realtime_task,
        );

        let backfill_tasks = backfill_tasks.to_vec();
        let config_clone = config.clone();
        let pool_clone = self.pool.clone();
        let bridge_genesis_checkpoint = self.bridge_genesis_checkpoint;
        let handle = spawn_monitored_task!(async {
            for backfill_task in backfill_tasks {
                Self::start_executor(
                    pool_clone.clone(),
                    bridge_genesis_checkpoint,
                    ingestion_metrics.clone(),
                    indexer_meterics.clone(),
                    &config_clone,
                    &backfill_task,
                )
                .await
                .expect("Backfill task failed");
            }
        });
        realtime_task_future.await?;
        tokio::try_join!(handle)?;
        Ok(())
    }

    pub fn tasks(&self) -> Result<Vec<Task>, anyhow::Error> {
        let mut conn = self.pool.get()?;
        // clean up completed task
        delete(dsl::progress_store.filter(columns::checkpoint.ge(columns::target_checkpoint)))
            .execute(&mut conn)?;
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store
            .order_by(columns::checkpoint.desc())
            .load(&mut conn)?;
        Ok(cp.into_iter().map(|d| d.into()).collect())
    }

    pub fn register_task(
        &self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: i64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: checkpoint as i64,
                target_checkpoint,
                timestamp: None,
            })
            .execute(&mut conn)?;
        Ok(())
    }

    pub fn update_task(&self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::update(dsl::progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }

    async fn start_executor(
        pool: PgPool,
        bridge_genesis_checkpoint: u64,
        ingestion_metrics: DataIngestionMetrics,
        indexer_meterics: BridgeIndexerMetrics,
        config: &IndexerConfig,
        task: &Task,
    ) -> anyhow::Result<()> {
        let (exit_sender, exit_receiver) = oneshot::channel();

        let progress_store = PgProgressStore {
            pool: pool.clone(),
            bridge_genesis_checkpoint,
            exit_checkpoint: task.target_checkpoint,
            exit_sender: Some(exit_sender),
        };

        let mut executor = IndexerExecutor::new(
            progress_store,
            1, /* workflow types */
            ingestion_metrics,
        );

        let indexer_metrics_cloned = indexer_meterics.clone();

        let worker = SuiBridgeWorker::new(vec![], pool, indexer_metrics_cloned);
        let worker_pool =
            WorkerPool::new(worker, task.task_name.clone(), config.concurrency as usize);
        executor.register(worker_pool).await?;
        executor
            .run(
                config.checkpoints_path.clone().into(),
                Some(config.remote_store_url.clone()),
                vec![], // optional remote store access options
                ReaderOptions::default(),
                exit_receiver,
            )
            .await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct Task {
    pub task_name: String,
    pub checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
}

impl From<models::ProgressStore> for Task {
    fn from(value: models::ProgressStore) -> Self {
        Self {
            task_name: value.task_name,
            checkpoint: value.checkpoint as u64,
            target_checkpoint: value.target_checkpoint as u64,
            // Ok to unwrap, timestamp is defaulted to now() in database
            timestamp: value.timestamp.expect("Timestamp not set").0 as u64,
        }
    }
}

pub trait Tasks {
    fn latest_checkpoint_task(&self) -> Option<Task>;
}

impl Tasks for Vec<Task> {
    fn latest_checkpoint_task(&self) -> Option<Task> {
        self.iter().fold(None, |result, other_task| match &result {
            Some(task) if task.checkpoint < other_task.checkpoint => Some(other_task.clone()),
            None => Some(other_task.clone()),
            _ => result,
        })
    }
}

pub struct PgProgressStore {
    pool: PgPool,
    bridge_genesis_checkpoint: u64,
    exit_checkpoint: u64,
    exit_sender: Option<Sender<()>>,
}

#[async_trait]
impl ProgressStore for PgProgressStore {
    async fn load(&mut self, task_name: String) -> anyhow::Result<CheckpointSequenceNumber> {
        let mut conn = self.pool.get()?;
        let cp: Option<models::ProgressStore> = dsl::progress_store
            .find(task_name)
            .select(models::ProgressStore::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(cp
            .map(|d| d.checkpoint as u64)
            .unwrap_or(self.bridge_genesis_checkpoint))
    }

    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        if checkpoint_number >= self.exit_checkpoint {
            if let Some(sender) = self.exit_sender.take() {
                let _ = sender.send(());
            }
        }
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(&models::ProgressStore {
                task_name,
                checkpoint: checkpoint_number as i64,
                target_checkpoint: i64::MAX,
                timestamp: None,
            })
            .on_conflict(dsl::task_name)
            .do_update()
            .set((
                columns::checkpoint.eq(checkpoint_number as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }
}

fn new_task_name() -> String {
    format!("bridge worker - {}", TransactionDigest::random())
}
