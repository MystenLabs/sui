// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::upsert::excluded;
use diesel::{ExpressionMethods, QueryDsl, TextExpressionMethods};
use diesel::{OptionalExtension, SelectableHelper};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;

use crate::models::ProgressStore;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::schema::{sui_error_transactions, token_transfer, token_transfer_data};
use crate::{schema, ProcessedTxnData};
use sui_indexer_builder::indexer_builder::{IndexerProgressStore, Persistent};
use sui_indexer_builder::{
    progress::ProgressSavingPolicy, Task, Tasks, LIVE_TASK_TARGET_CHECKPOINT,
};

/// Persistent layer impl
#[derive(Clone)]
pub struct PgBridgePersistent {
    pool: PgPool,
    save_progress_policy: ProgressSavingPolicy,
}

impl PgBridgePersistent {
    pub fn new(pool: PgPool, save_progress_policy: ProgressSavingPolicy) -> Self {
        Self {
            pool,
            save_progress_policy,
        }
    }

    async fn get_largest_backfill_task_target_checkpoint(
        &self,
        prefix: &str,
    ) -> Result<Option<u64>, Error> {
        use schema::progress_store::dsl::*;
        let mut conn = self.pool.get().await?;
        let cp = progress_store
            // TODO: using like could be error prone, change the progress store schema to store the task name properly.
            .filter(task_name.like(format!("{prefix} - %")))
            .filter(target_checkpoint.ne(i64::MAX))
            .select(target_checkpoint)
            .order_by(target_checkpoint.desc())
            .first::<i64>(&mut conn)
            .await
            .optional()?;
        Ok(cp.map(|c| c as u64))
    }
}

#[async_trait]
impl Persistent<ProcessedTxnData> for PgBridgePersistent {
    async fn write(&self, data: Vec<ProcessedTxnData>) -> Result<(), Error> {
        use diesel::query_dsl::methods::FilterDsl;
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
                                    .on_conflict((
                                        token_transfer::dsl::chain_id,
                                        token_transfer::dsl::nonce,
                                        token_transfer::dsl::status,
                                    ))
                                    .do_update()
                                    .set((
                                        token_transfer::chain_id
                                            .eq(excluded(token_transfer::chain_id)),
                                        token_transfer::nonce.eq(excluded(token_transfer::nonce)),
                                        token_transfer::status.eq(excluded(token_transfer::status)),
                                        token_transfer::block_height
                                            .eq(excluded(token_transfer::block_height)),
                                        token_transfer::timestamp_ms
                                            .eq(excluded(token_transfer::timestamp_ms)),
                                        token_transfer::txn_hash
                                            .eq(excluded(token_transfer::txn_hash)),
                                        token_transfer::txn_sender
                                            .eq(excluded(token_transfer::txn_sender)),
                                        token_transfer::gas_usage
                                            .eq(excluded(token_transfer::gas_usage)),
                                        token_transfer::data_source
                                            .eq(excluded(token_transfer::data_source)),
                                        token_transfer::is_finalized
                                            .eq(excluded(token_transfer::is_finalized)),
                                    ))
                                    .filter(token_transfer::is_finalized.eq(false))
                                    .execute(conn)
                                    .await?;

                                if let Some(d) = t.to_data_maybe() {
                                    diesel::insert_into(token_transfer_data::table)
                                        .values(&d)
                                        .on_conflict((
                                            token_transfer_data::dsl::chain_id,
                                            token_transfer_data::dsl::nonce,
                                        ))
                                        .do_update()
                                        .set((
                                            token_transfer_data::chain_id
                                                .eq(excluded(token_transfer_data::chain_id)),
                                            token_transfer_data::nonce
                                                .eq(excluded(token_transfer_data::nonce)),
                                            token_transfer_data::block_height
                                                .eq(excluded(token_transfer_data::block_height)),
                                            token_transfer_data::timestamp_ms
                                                .eq(excluded(token_transfer_data::timestamp_ms)),
                                            token_transfer_data::txn_hash
                                                .eq(excluded(token_transfer_data::txn_hash)),
                                            token_transfer_data::sender_address
                                                .eq(excluded(token_transfer_data::sender_address)),
                                            token_transfer_data::destination_chain.eq(excluded(
                                                token_transfer_data::destination_chain,
                                            )),
                                            token_transfer_data::recipient_address.eq(excluded(
                                                token_transfer_data::recipient_address,
                                            )),
                                            token_transfer_data::token_id
                                                .eq(excluded(token_transfer_data::token_id)),
                                            token_transfer_data::amount
                                                .eq(excluded(token_transfer_data::amount)),
                                            token_transfer_data::is_finalized
                                                .eq(excluded(token_transfer_data::is_finalized)),
                                        ))
                                        .filter(token_transfer_data::is_finalized.eq(false))
                                        .execute(conn)
                                        .await?;
                                }
                            }
                            ProcessedTxnData::GovernanceAction(a) => {
                                diesel::insert_into(schema::governance_actions::table)
                                    .values(&a.to_db())
                                    .on_conflict_do_nothing()
                                    .execute(conn)
                                    .await?;
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
        let cp = dsl::progress_store
            .find(&task_name)
            .select(ProgressStore::as_select())
            .first(&mut conn)
            .await
            .optional()?;
        Ok(cp
            .ok_or(anyhow!("Cannot found progress for task {task_name}"))?
            .checkpoint as u64)
    }

    async fn save_progress(
        &mut self,
        task: &Task,
        checkpoint_numbers: &[u64],
    ) -> anyhow::Result<Option<u64>> {
        if checkpoint_numbers.is_empty() {
            return Ok(None);
        }
        let task_name = task.task_name.clone();
        if let Some(checkpoint_to_save) = self
            .save_progress_policy
            .cache_progress(task, checkpoint_numbers)
        {
            let mut conn = self.pool.get().await?;
            diesel::insert_into(schema::progress_store::table)
                .values(&ProgressStore {
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
            return Ok(Some(checkpoint_to_save));
        }
        Ok(None)
    }

    async fn get_ongoing_tasks(&self, prefix: &str) -> Result<Tasks, Error> {
        use schema::progress_store::dsl::*;
        let mut conn = self.pool.get().await?;
        // get all unfinished tasks
        let cp = progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(task_name.like(format!("{prefix} - %")))
            .filter(checkpoint.lt(target_checkpoint))
            .order_by(target_checkpoint.desc())
            .load::<ProgressStore>(&mut conn)
            .await?;
        let tasks = cp.into_iter().map(|d| d.into()).collect();
        Ok(Tasks::new(tasks)?)
    }

    async fn get_largest_indexed_checkpoint(&self, prefix: &str) -> Result<Option<u64>, Error> {
        use schema::progress_store::dsl::*;
        let mut conn = self.pool.get().await?;
        let cp = progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(task_name.like(format!("{prefix} - %")))
            .filter(target_checkpoint.eq(i64::MAX))
            .select(checkpoint)
            .first::<i64>(&mut conn)
            .await
            .optional()?;

        if let Some(cp) = cp {
            Ok(Some(cp as u64))
        } else {
            // Use the largest backfill target checkpoint as a fallback
            self.get_largest_backfill_task_target_checkpoint(prefix)
                .await
        }
    }

    /// Register a new task to progress store with a start checkpoint and target checkpoint.
    /// Usually used for backfill tasks.
    async fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(ProgressStore {
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
    ) -> Result<(), Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(ProgressStore {
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
        diesel::update(QueryDsl::filter(
            dsl::progress_store,
            columns::task_name.eq(task.task_name),
        ))
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
