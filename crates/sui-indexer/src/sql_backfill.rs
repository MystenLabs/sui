// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::SqlBackFillConfig;
use crate::database::ConnectionPool;
use diesel_async::RunQueryDsl;
use futures::{stream, StreamExt};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

pub async fn run_sql_backfill(
    sql: &str,
    checkpoint_column_name: &str,
    first_checkpoint: u64,
    last_checkpoint: u64,
    pool: ConnectionPool,
    backfill_config: SqlBackFillConfig,
) {
    let cur_time = Instant::now();
    // Keeps track of the checkpoint ranges (using starting checkpoint number)
    // that are in progress.
    let in_progress = Arc::new(Mutex::new(BTreeSet::new()));
    let chunks: Vec<(u64, u64)> = (first_checkpoint..=last_checkpoint)
        .step_by(backfill_config.chunk_size)
        .map(|chunk_start| {
            let chunk_end = std::cmp::min(
                chunk_start + backfill_config.chunk_size as u64 - 1,
                last_checkpoint,
            );
            (chunk_start, chunk_end)
        })
        .collect();

    stream::iter(chunks)
        .for_each_concurrent(backfill_config.max_concurrency, |(start_id, end_id)| {
            let pool_clone = pool.clone(); // Clone the pool for async operation
            let in_progress_clone = in_progress.clone();
            async move {
                in_progress_clone.lock().await.insert(start_id);
                // Run the copy in a batch and add a delay
                backfill_data_batch(sql, checkpoint_column_name, start_id, end_id, pool_clone)
                    .await;
                println!("Finished checkpoint range: {} - {}.", start_id, end_id);
                in_progress_clone.lock().await.remove(&start_id);
                let cur_min_in_progress = in_progress_clone.lock().await.iter().next().cloned();
                println!(
                    "Minimum checkpoint number still in progress: {:?}.\
                    If the binary ever fails, you can restart from this checkpoint",
                    cur_min_in_progress
                );
            }
        })
        .await;
    println!("Finished backfilling in {:?}", cur_time.elapsed());
}

async fn backfill_data_batch(
    sql: &str,
    checkpoint_column_name: &str,
    first_checkpoint: u64,
    last_checkpoint: u64,
    pool: ConnectionPool,
) {
    let mut conn = pool.get().await.unwrap();

    let query = format!(
        "{} WHERE {} BETWEEN {} AND {} ON CONFLICT DO NOTHING",
        sql, checkpoint_column_name, first_checkpoint, last_checkpoint
    );

    // Execute the SQL query using Diesel's async connection
    // TODO: Add retry support.
    diesel::sql_query(query).execute(&mut conn).await.unwrap();
}
