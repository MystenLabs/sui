// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::IndexerConfig;
use crate::deepbook::deepbook_worker::DeepbookWorker;
use crate::deepbook::metrics::DeepbookIndexerMetrics;
use crate::models;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{self, columns, dsl};
use async_trait::async_trait;
use chrono::Utc;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper};
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, WorkerPool,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tracing::info;

pub struct SuiCheckpointSyncer {
    pool: PgPool,
    start_checkpoint: u64,
    end_checkpoint: u64,
}

impl SuiCheckpointSyncer {
    pub fn new(pool: PgPool, start_checkpoint: u64, end_checkpoint: u64) -> Self {
        SuiCheckpointSyncer {
            pool: pool.clone(),
            start_checkpoint,
            end_checkpoint,
        }
    }

    pub async fn start(
        self,
        config: &IndexerConfig,
        indexer_meterics: DeepbookIndexerMetrics,
        ingestion_metrics: DataIngestionMetrics,
    ) -> anyhow::Result<(), anyhow::Error> {
        info!("Start executor: {:?}", config);
        let mut tasks = self.tasks()?;
        if tasks.is_empty() {
            tasks.push(Task {
                task_name: "live".to_string(),
                current_checkpoint: self.start_checkpoint,
                target_checkpoint: self.end_checkpoint,
                timestamp: 0,
            });
        }
        let task = Self::start_executor(
            self.pool.clone(),
            ingestion_metrics,
            indexer_meterics,
            config,
            &tasks[0],
        );
        task.await?;

        Ok(())
    }

    pub fn tasks(&self) -> Result<Vec<Task>, anyhow::Error> {
        let mut conn = self.pool.get()?;
        // clean up completed task
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store.load(&mut conn)?;
        Ok(cp.into_iter().map(|d| d.into()).collect())
    }

    async fn start_executor(
        pool: PgPool,
        ingestion_metrics: DataIngestionMetrics,
        indexer_meterics: DeepbookIndexerMetrics,
        config: &IndexerConfig,
        task: &Task,
    ) -> anyhow::Result<()> {
        info!("Start executor");
        let (exit_sender, exit_receiver) = oneshot::channel();

        let ps = PgProgressStore {
            pool: pool.clone(),
            start_checkpoint: task.current_checkpoint,
            end_checkpoint: task.target_checkpoint,
            exit_sender: Some(exit_sender),
        };

        let mut executor = IndexerExecutor::new(ps, 1 /* workflow types */, ingestion_metrics);

        let indexer_metrics_cloned = indexer_meterics.clone();

        let object_types = vec![
            "d8bb402e5ee5e9d59b720077a14ad19cb51f5cbd53a7d2794eb5f03c212311d0::pool::Pool"
                .to_string(),
        ];
        let worker = DeepbookWorker::new(object_types, pool, indexer_metrics_cloned);
        let worker_pool =
            WorkerPool::new(worker, task.task_name.clone(), config.concurrency as usize);
        executor.register(worker_pool).await?;
        info!("Start executor: worker pool ready");
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

#[derive(Clone, Debug)]
pub struct Task {
    pub task_name: String,
    pub current_checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
}

impl From<models::ProgressStore> for Task {
    fn from(ps: models::ProgressStore) -> Self {
        Task {
            task_name: ps.task_name,
            current_checkpoint: ps.checkpoint as u64,
            target_checkpoint: ps.target_checkpoint as u64,
            timestamp: ps.timestamp as u64,
        }
    }
}

pub struct PgProgressStore {
    pool: PgPool,
    start_checkpoint: u64,
    end_checkpoint: u64,
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
            .unwrap_or(self.start_checkpoint))
    }

    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        if checkpoint_number >= self.end_checkpoint {
            if let Some(sender) = self.exit_sender.take() {
                let _ = sender.send(());
            }
        }
        let mut conn = self.pool.get()?;
        diesel::insert_into(progress_store::table)
            .values(&models::ProgressStore {
                task_name,
                checkpoint: checkpoint_number as i64,
                target_checkpoint: self.end_checkpoint as i64,
                timestamp: 0,
            })
            .on_conflict(dsl::task_name)
            .do_update()
            .set((
                columns::checkpoint.eq(checkpoint_number as i64),
                columns::timestamp.eq(Utc::now().timestamp_millis()),
            ))
            .execute(&mut conn)?;
        Ok(())
    }
}
