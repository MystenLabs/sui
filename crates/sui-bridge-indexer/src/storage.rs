// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)] // TODO: remove in next PR where integration of ProgressSavingPolicy is done

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::{ExpressionMethods, TextExpressionMethods};
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;

use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::schema::{sui_error_transactions, token_transfer, token_transfer_data};
use crate::{models, schema, ProcessedTxnData};
use sui_indexer_builder::indexer_builder::{IndexerProgressStore, Persistent};
use sui_indexer_builder::Task;

/// Persistent layer impl
#[derive(Clone)]
pub struct PgBridgePersistent {
    pool: PgPool,
}

impl PgBridgePersistent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
        checkpoint_number: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(&models::ProgressStore {
                task_name,
                checkpoint: checkpoint_number as i64,
                // Target checkpoint and timestamp will only be written for new entries
                target_checkpoint: i64::MAX,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .on_conflict(dsl::task_name)
            .do_update()
            .set((
                columns::checkpoint.eq(checkpoint_number as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    async fn get_ongoing_tasks(&self, prefix: &str) -> Result<Vec<Task>, anyhow::Error> {
        let mut conn = self.pool.get().await?;
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::checkpoint.lt(columns::target_checkpoint))
            .order_by(columns::target_checkpoint.desc())
            .load(&mut conn)
            .await?;
        Ok(cp.into_iter().map(|d| d.into()).collect())
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

    async fn update_task(&mut self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::update(dsl::progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ProgressSavingPolicy {
    SaveAfterDuration(SaveAfterDurationPolicy),
    OutOfOrderSaveAfterDuration(OutOfOrderSaveAfterDurationPolicy),
}

#[derive(Debug, Clone)]
pub struct SaveAfterDurationPolicy {
    duration: tokio::time::Duration,
    last_save_time: Arc<Mutex<HashMap<String, Option<tokio::time::Instant>>>>,
}

impl SaveAfterDurationPolicy {
    pub fn new(duration: tokio::time::Duration) -> Self {
        Self {
            duration,
            last_save_time: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutOfOrderSaveAfterDurationPolicy {
    duration: tokio::time::Duration,
    last_save_time: Arc<Mutex<HashMap<String, Option<tokio::time::Instant>>>>,
    seen: Arc<Mutex<HashMap<String, HashSet<u64>>>>,
    next_to_fill: Arc<Mutex<HashMap<String, Option<u64>>>>,
}

impl OutOfOrderSaveAfterDurationPolicy {
    pub fn new(duration: tokio::time::Duration) -> Self {
        Self {
            duration,
            last_save_time: Arc::new(Mutex::new(HashMap::new())),
            seen: Arc::new(Mutex::new(HashMap::new())),
            next_to_fill: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ProgressSavingPolicy {
    /// If returns Some(progress), it means we should save the progress to DB.
    fn cache_progress(
        &mut self,
        task_name: String,
        heights: &[u64],
        start_height: u64,
        target_height: u64,
    ) -> Option<u64> {
        match self {
            ProgressSavingPolicy::SaveAfterDuration(policy) => {
                let height = *heights.iter().max().unwrap();
                let mut last_save_time_guard = policy.last_save_time.lock().unwrap();
                let last_save_time = last_save_time_guard.entry(task_name).or_insert(None);
                if height >= target_height {
                    *last_save_time = Some(tokio::time::Instant::now());
                    return Some(height);
                }
                if let Some(v) = last_save_time {
                    if v.elapsed() >= policy.duration {
                        *last_save_time = Some(tokio::time::Instant::now());
                        Some(height)
                    } else {
                        None
                    }
                } else {
                    // update `last_save_time` to now but don't actually save progress
                    *last_save_time = Some(tokio::time::Instant::now());
                    None
                }
            }
            ProgressSavingPolicy::OutOfOrderSaveAfterDuration(policy) => {
                let mut next_to_fill = {
                    let mut next_to_fill_guard = policy.next_to_fill.lock().unwrap();
                    (*next_to_fill_guard
                        .entry(task_name.clone())
                        .or_insert(Some(start_height)))
                    .unwrap()
                };
                let old_next_to_fill = next_to_fill;
                {
                    let mut seen_guard = policy.seen.lock().unwrap();
                    let seen = seen_guard
                        .entry(task_name.clone())
                        .or_insert(HashSet::new());
                    seen.extend(heights.iter().cloned());
                    while seen.remove(&next_to_fill) {
                        next_to_fill += 1;
                    }
                }
                // We made some progress in filling gaps
                if old_next_to_fill != next_to_fill {
                    policy
                        .next_to_fill
                        .lock()
                        .unwrap()
                        .insert(task_name.clone(), Some(next_to_fill));
                }

                let mut last_save_time_guard = policy.last_save_time.lock().unwrap();
                let last_save_time = last_save_time_guard
                    .entry(task_name.clone())
                    .or_insert(None);

                // If we have reached the target height, we always save
                if next_to_fill > target_height {
                    *last_save_time = Some(tokio::time::Instant::now());
                    return Some(next_to_fill - 1);
                }
                // Regardless of whether we made progress, we should save if we have waited long enough
                if let Some(v) = last_save_time {
                    if v.elapsed() >= policy.duration && next_to_fill > start_height {
                        *last_save_time = Some(tokio::time::Instant::now());
                        Some(next_to_fill - 1)
                    } else {
                        None
                    }
                } else {
                    // update `last_save_time` to now but don't actually save progress
                    *last_save_time = Some(tokio::time::Instant::now());
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_save_after_duration_policy() {
        let duration = tokio::time::Duration::from_millis(100);
        let mut policy =
            ProgressSavingPolicy::SaveAfterDuration(SaveAfterDurationPolicy::new(duration));
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[1], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[2], 0, 100),
            Some(2)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[3], 0, 100),
            Some(3)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[4], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[5, 6], 0, 100),
            Some(6)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[8, 7], 0, 100),
            Some(8)
        );
    }

    #[tokio::test]
    async fn test_out_of_order_save_after_duration_policy() {
        let duration = tokio::time::Duration::from_millis(100);
        let mut policy = ProgressSavingPolicy::OutOfOrderSaveAfterDuration(
            OutOfOrderSaveAfterDurationPolicy::new(duration),
        );

        assert_eq!(
            policy.cache_progress("task1".to_string(), &[0], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[1], 0, 100),
            Some(1)
        );
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[3], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[4], 0, 100),
            Some(1)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[2], 0, 100),
            Some(4)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[0], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[1], 0, 100),
            Some(1)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[2], 0, 100),
            Some(2)
        );
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[3], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[4], 0, 100),
            Some(4)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[6, 7, 8], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[5, 9], 0, 100),
            Some(9)
        );
    }
}
