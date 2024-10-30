// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::get_backfill_task;
use crate::backfill::backfill_task::BackfillTask;
use crate::backfill::BackfillTaskKind;
use crate::config::BackFillConfig;
use crate::database::ConnectionPool;
use futures::StreamExt;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;

pub struct BackfillRunner {}

impl BackfillRunner {
    pub async fn run(
        runner_kind: BackfillTaskKind,
        pool: ConnectionPool,
        backfill_config: BackFillConfig,
        total_range: RangeInclusive<usize>,
    ) {
        let task = get_backfill_task(runner_kind, *total_range.start()).await;
        Self::run_impl(pool, backfill_config, total_range, task).await;
    }

    /// Main function to run the parallel queries and batch processing.
    async fn run_impl(
        pool: ConnectionPool,
        config: BackFillConfig,
        total_range: RangeInclusive<usize>,
        task: Arc<dyn BackfillTask>,
    ) {
        let cur_time = Instant::now();
        // Keeps track of the checkpoint ranges (using starting checkpoint number)
        // that are in progress.
        let in_progress = Arc::new(Mutex::new(BTreeSet::new()));

        let concurrency = config.max_concurrency;
        let (tx, rx) = mpsc::channel(concurrency * 10);
        // Spawn a task to send chunks lazily over the channel
        tokio::spawn(async move {
            for chunk in create_chunk_iter(total_range, config.chunk_size) {
                if tx.send(chunk).await.is_err() {
                    // Channel closed, stop producing chunks
                    break;
                }
            }
        });
        // Convert the receiver into a stream
        let stream = ReceiverStream::new(rx);

        // Process chunks in parallel, limiting the number of concurrent query tasks
        stream
            .for_each_concurrent(concurrency, move |range| {
                let pool_clone = pool.clone();
                let in_progress_clone = in_progress.clone();
                let task = task.clone();

                async move {
                    in_progress_clone.lock().await.insert(*range.start());
                    task.backfill_range(pool_clone, &range).await;
                    println!("Finished range: {:?}.", range);
                    in_progress_clone.lock().await.remove(range.start());
                    let cur_min_in_progress = in_progress_clone.lock().await.iter().next().cloned();
                    if let Some(cur_min_in_progress) = cur_min_in_progress {
                        println!(
                            "Average backfill speed: {} checkpoints/s. Minimum range start number still in progress: {:?}.",
                            cur_min_in_progress as f64 / cur_time.elapsed().as_secs_f64(),
                            cur_min_in_progress
                        );
                    }
                }
            })
            .await;

        println!("Finished backfilling in {:?}", cur_time.elapsed());
    }
}

/// Creates chunks based on the total range and chunk size.
fn create_chunk_iter(
    total_range: RangeInclusive<usize>,
    chunk_size: usize,
) -> impl Iterator<Item = RangeInclusive<usize>> {
    let end = *total_range.end();
    total_range.step_by(chunk_size).map(move |chunk_start| {
        let chunk_end = std::cmp::min(chunk_start + chunk_size - 1, end);
        chunk_start..=chunk_end
    })
}
