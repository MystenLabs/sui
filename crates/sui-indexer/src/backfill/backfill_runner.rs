// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::full_objects_history::FullObjectsHistoryBackfill;
use crate::backfill::backfill_instances::system_state_summary_json::SystemStateSummaryJsonBackfill;
use crate::backfill::backfill_task::BackfillTask;
use crate::backfill::BackfillTaskKind;
use crate::config::BackFillConfig;
use crate::database::ConnectionPool;
use futures::StreamExt;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

pub struct BackfillRunner {}

impl BackfillRunner {
    pub async fn run(
        runner_kind: BackfillTaskKind,
        pool: ConnectionPool,
        backfill_config: BackFillConfig,
        total_range: RangeInclusive<usize>,
    ) {
        match runner_kind {
            BackfillTaskKind::FullObjectsHistory => {
                Self::run_impl::<FullObjectsHistoryBackfill>(pool, backfill_config, total_range)
                    .await;
            }
            BackfillTaskKind::SystemStateSummaryJson => {
                Self::run_impl::<SystemStateSummaryJsonBackfill>(
                    pool,
                    backfill_config,
                    total_range,
                )
                .await;
            }
        }
    }

    /// Main function to run the parallel queries and batch processing.
    async fn run_impl<T: BackfillTask>(
        pool: ConnectionPool,
        config: BackFillConfig,
        total_range: RangeInclusive<usize>,
    ) {
        let cur_time = Instant::now();
        // Keeps track of the checkpoint ranges (using starting checkpoint number)
        // that are in progress.
        let in_progress = Arc::new(Mutex::new(BTreeSet::new()));

        // Generate chunks from the total range
        let chunks = create_chunks(total_range.clone(), config.chunk_size);

        // Create a stream from the ranges
        let stream = tokio_stream::iter(chunks);

        let concurrency = config.max_concurrency;

        // Process chunks in parallel, limiting the number of concurrent query tasks
        stream
            .for_each_concurrent(concurrency, move |range| {
                let pool_clone = pool.clone();
                let in_progress_clone = in_progress.clone();

                async move {
                    in_progress_clone.lock().await.insert(*range.start());
                    T::backfill_range(pool_clone, &range).await;
                    println!("Finished range: {:?}.", range);
                    in_progress_clone.lock().await.remove(range.start());
                    let cur_min_in_progress = in_progress_clone.lock().await.iter().next().cloned();
                    println!(
                        "Minimum range start number still in progress: {:?}.",
                        cur_min_in_progress
                    );
                }
            })
            .await;

        println!("Finished backfilling in {:?}", cur_time.elapsed());
    }
}

/// Creates chunks based on the total range and chunk size.
fn create_chunks(
    total_range: RangeInclusive<usize>,
    chunk_size: usize,
) -> Vec<RangeInclusive<usize>> {
    let end = *total_range.end();
    total_range
        .step_by(chunk_size)
        .map(|chunk_start| {
            let chunk_end = std::cmp::min(chunk_start + chunk_size - 1, end);
            chunk_start..=chunk_end
        })
        .collect()
}
