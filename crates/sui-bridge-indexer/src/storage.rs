// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::{ExpressionMethods, TextExpressionMethods};
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;

use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::schema::{sui_error_transactions, token_transfer, token_transfer_data};
use crate::{models, schema, ProcessedTxnData};
use sui_indexer_builder::indexer_builder::{IndexerProgressStore, Persistent};
use sui_indexer_builder::{
    progress::ProgressSavingPolicy, Task, Tasks, LIVE_TASK_TARGET_CHECKPOINT,
};

/// Persistent layer impl
#[derive(Clone)]
pub struct PgBridgePersistent {
    pool: PgPool,
    save_progress_policy: ProgressSavingPolicy,
    indexer_metrics: BridgeIndexerMetrics,
}

impl PgBridgePersistent {
    pub fn new(
        pool: PgPool,
        save_progress_policy: ProgressSavingPolicy,
        indexer_metrics: BridgeIndexerMetrics,
    ) -> Self {
        Self {
            pool,
            save_progress_policy,
            indexer_metrics,
        }
    }
}

#[async_trait]
impl Persistent<ProcessedTxnData> for PgBridgePersistent {
    async fn write(&self, data: Vec<ProcessedTxnData>) -> Result<(), Error> {
        if data.is_empty() {
            return Ok(());
        }
        let connection = &mut self.pool.get().await?;
        connection
            .transaction(|conn| {
                async move {
                    for d in data {
                        match d {
                            ProcessedTxnData::TokenTransfer(t) => {
                                diesel::insert_into(token_transfer::table)
                                    .values(&t.to_db())
                                    .on_conflict_do_nothing()
                                    .execute(conn)
                                    .await?;

                                if let Some(d) = t.to_data_maybe() {
                                    diesel::insert_into(token_transfer_data::table)
                                        .values(&d)
                                        .on_conflict_do_nothing()
                                        .execute(conn)
                                        .await?;
                                }
                            }
                            ProcessedTxnData::Error(e) => {
                                diesel::insert_into(sui_error_transactions::table)
                                    .values(&e.to_db())
                                    .on_conflict_do_nothing()
                                    .execute(conn)
                                    .await?;
                            }
                        }
                    }
                    Ok(())
                }
                .scope_boxed()
            })
            .await
    }
}

#[async_trait]
impl IndexerProgressStore for PgBridgePersistent {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64> {
        let mut conn = self.pool.get().await?;
        let cp: Option<models::ProgressStore> = dsl::progress_store
            .find(&task_name)
            .select(models::ProgressStore::as_select())
            .first(&mut conn)
            .await
            .optional()?;
        Ok(cp
            .ok_or(anyhow!("Cannot found progress for task {task_name}"))?
            .checkpoint as u64)
    }

    async fn save_progress(
        &mut self,
        task_name: String,
        checkpoint_numbers: &[u64],
        start_checkpoint_number: u64,
        target_checkpoint_number: u64,
    ) -> anyhow::Result<Option<u64>> {
        if checkpoint_numbers.is_empty() {
            return Ok(None);
        }
        if let Some(checkpoint_to_save) = self.save_progress_policy.cache_progress(
            task_name.clone(),
            checkpoint_numbers,
            start_checkpoint_number,
            target_checkpoint_number,
        ) {
            let mut conn = self.pool.get().await?;
            diesel::insert_into(schema::progress_store::table)
                .values(&models::ProgressStore {
                    task_name: task_name.clone(),
                    checkpoint: checkpoint_to_save as i64,
                    // Target checkpoint and timestamp will only be written for new entries
                    target_checkpoint: i64::MAX,
                    // Timestamp is defaulted to current time in DB if None
                    timestamp: None,
                })
                .on_conflict(dsl::task_name)
                .do_update()
                .set((
                    columns::checkpoint.eq(checkpoint_to_save as i64),
                    columns::timestamp.eq(now),
                ))
                .execute(&mut conn)
                .await?;
            self.indexer_metrics
                .tasks_current_checkpoints
                .with_label_values(&[&task_name])
                .set(checkpoint_to_save as i64);
            return Ok(Some(checkpoint_to_save));
        }
        Ok(None)
    }

    async fn get_ongoing_tasks(&self, prefix: &str) -> Result<Tasks, anyhow::Error> {
        let mut conn = self.pool.get().await?;
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::checkpoint.lt(columns::target_checkpoint))
            .order_by(columns::target_checkpoint.desc())
            .load(&mut conn)
            .await?;
        let tasks = cp.into_iter().map(|d| d.into()).collect();
        Ok(Tasks::new(tasks)?)
    }

    async fn get_largest_backfill_task_target_checkpoint(
        &self,
        prefix: &str,
    ) -> Result<Option<u64>, Error> {
        let mut conn = self.pool.get().await?;
        let cp: Option<i64> = dsl::progress_store
            .select(columns::target_checkpoint)
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::target_checkpoint.ne(i64::MAX))
            .order_by(columns::target_checkpoint.desc())
            .first::<i64>(&mut conn)
            .await
            .optional()?;
        Ok(cp.map(|c| c as u64))
    }

    /// Register a new task to progress store with a start checkpoint and target checkpoint.
    /// Usually used for backfill tasks.
    async fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: checkpoint as i64,
                target_checkpoint: target_checkpoint as i64,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    /// Register a live task to progress store with a start checkpoint.
    async fn register_live_task(
        &mut self,
        task_name: String,
        start_checkpoint: u64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: start_checkpoint as i64,
                target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    async fn update_task(&mut self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::update(dsl::progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.start_checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)
            .await?;
        Ok(())
    }
}
