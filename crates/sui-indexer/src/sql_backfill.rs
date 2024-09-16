// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::ConnectionPool;
use diesel_async::RunQueryDsl;
use futures::{stream, StreamExt};
use std::time::Instant;

const CHUNK_SIZE: u64 = 10000;
const MAX_CONCURRENCY: usize = 100;

pub async fn run_sql_backfill(
    sql: &str,
    checkpoint_column_name: &str,
    first_checkpoint: u64,
    last_checkpoint: u64,
    pool: ConnectionPool,
) {
    let cur_time = Instant::now();
    let chunks: Vec<(u64, u64)> = (first_checkpoint..=last_checkpoint)
        .step_by(CHUNK_SIZE as usize)
        .map(|chunk_start| {
            let chunk_end = std::cmp::min(chunk_start + CHUNK_SIZE - 1, last_checkpoint);
            (chunk_start, chunk_end)
        })
        .collect();

    stream::iter(chunks)
        .for_each_concurrent(MAX_CONCURRENCY, |(start_id, end_id)| {
            let pool_clone = pool.clone(); // Clone the pool for async operation
            async move {
                // Run the copy in a batch and add a delay
                backfill_data_batch(sql, checkpoint_column_name, start_id, end_id, pool_clone)
                    .await;
                println!("Finished checkpoint range: {} - {}", start_id, end_id);
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
        "{} WHERE {} BETWEEN {} AND {}",
        sql, checkpoint_column_name, first_checkpoint, last_checkpoint
    );

    // Execute the SQL query using Diesel's async connection
    // TODO: Add retry support.
    diesel::sql_query(query).execute(&mut conn).await.unwrap();
}
