// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use diesel::{
    prelude::QueryableByName, query_dsl::methods::FilterDsl, sql_types::BigInt, ExpressionMethods,
};
use diesel_async::RunQueryDsl;
use futures::{stream::FuturesUnordered, StreamExt};
use mysten_metrics::spawn_monitored_task;
use tokio::{sync::Semaphore, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    database::Connection, models::watermarks::StoredWatermark, schema::watermarks,
    store::PgIndexerStore, types::IndexerResult,
};

use super::pruner::PrunableTable;

// pub mod checkpoints;
// pub mod ev_emit_module;
// pub mod ev_emit_package;
// pub mod ev_senders;
// pub mod ev_struct_inst;
// pub mod ev_struct_module;
// pub mod ev_struct_name;
// pub mod ev_struct_package;
pub mod events;
pub mod objects_history;
pub mod transactions;
// pub mod tx_affected_addresses;
// pub mod tx_affected_objects;
// pub mod tx_calls_fun;
// pub mod tx_calls_mod;
// pub mod tx_calls_pkg;
// pub mod tx_changed_objects;
// pub mod tx_digests;
// pub mod tx_input_objects;
// pub mod tx_kinds;

/// Pruners implement the logic for a given table: How to fetch the earliest available data from the
/// table, and how to delete rows up to the pruner watermark.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values). Reasonable defaults have been chosen to balance concurrency with memory
/// usage, but each handle may choose to override these defaults, e.g.
///
/// - Handlers that produce many small rows may wish to increase their batch/chunk/max-pending
///   sizes).
/// - Handlers that do more work during processing may wish to increase their fanout so more of it
///   can be done concurrently, to preserve throughput.
#[async_trait::async_trait]
pub trait Prunable: Send {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: PrunableTable;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// How many rows to delete at once.
    const CHUNK_SIZE: u64 = 100000;

    /// Earliest available data in the table.
    async fn data_lo(conn: &mut Connection<'_>) -> anyhow::Result<u64>;

    /// Pruner hi watermark.
    async fn pruner_hi(conn: &mut Connection<'_>) -> anyhow::Result<u64> {
        let watermark = watermarks::table
            .filter(watermarks::pipeline.eq(Self::NAME.as_ref()))
            .first::<StoredWatermark>(conn)
            .await?;

        Ok(watermark.pruner_hi as u64)
    }

    /// Prune the table between `[prune_lo, prune_hi)`.
    async fn prune(
        prune_lo: u64,
        prune_hi: u64,
        conn: &mut Connection<'_>,
    ) -> anyhow::Result<usize>;
}

pub const NUM_WORKERS: usize = 5;

/// Handles concurrency and chunking for a table.
pub struct PrunerManager<T: Prunable> {
    cancel: CancellationToken,
    store: PgIndexerStore,
    semaphore: Arc<Semaphore>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Prunable> PrunerManager<T> {
    pub fn new(cancel: CancellationToken, store: PgIndexerStore) -> Self {
        Self {
            cancel,
            store,
            // The semaphore ensures we only have NUM_WORKERS active deletion
            // tasks at a time, even if we've queued up more chunks
            semaphore: Arc::new(Semaphore::new(NUM_WORKERS)),
            _phantom: std::marker::PhantomData,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        loop {
            if self.cancel.is_cancelled() {
                info!("Cancelling pruning task for {}", T::NAME.as_ref());
                return Ok(());
            }

            let pool = self.store.pool();
            let mut conn = pool.get().await?;

            let lo = T::data_lo(&mut conn).await?;
            let hi = T::pruner_hi(&mut conn).await?;

            if lo >= hi {
                return Ok(());
            }

            let mut futures = FuturesUnordered::new();
            let mut current_lo = lo;
            let mut total_pruned = 0;

            // Queue up all the chunks we need to process
            while current_lo < hi {
                let chunk_hi = (current_lo + T::CHUNK_SIZE).min(hi);
                let semaphore = self.semaphore.clone();
                let store_clone = self.store.clone();

                futures.push(tokio::spawn(async move {
                    let _permit = semaphore.acquire().await?;
                    let pool = store_clone.pool();
                    let mut conn = pool.get().await?;
                    T::prune(current_lo, chunk_hi, &mut conn).await
                }));

                current_lo = chunk_hi;
            }

            // Process results as they complete
            while let Some(result) = futures.next().await {
                total_pruned += result??;
            }
        }
    }
}

pub fn get_partition_sql(table_name: &str) -> String {
    format!(
        r"
        SELECT
            MIN(SUBSTRING(child.relname FROM '\d+$'))::integer as first_partition
        FROM pg_inherits
        JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
        JOIN pg_class child ON pg_inherits.inhrelid = child.oid
        WHERE parent.relname = '{}';
        ",
        table_name
    )
}

#[derive(QueryableByName, Debug, Clone)]
struct PartitionedTable {
    #[diesel(sql_type = BigInt)]
    first_partition: i64,
}

pub async fn run_pruner<T: Prunable>(
    cancel: CancellationToken,
    store: PgIndexerStore,
) -> IndexerResult<()> {
    // Create semaphore outside the loop since it's shared across iterations
    let semaphore = Arc::new(Semaphore::new(NUM_WORKERS));

    loop {
        if cancel.is_cancelled() {
            info!("Cancelling pruning task for {}", T::NAME.as_ref());
            return Ok(());
        }

        let pool = store.pool();
        let mut conn = pool.get().await?;

        let lo = T::data_lo(&mut conn).await?;
        let hi = T::pruner_hi(&mut conn).await?;

        if lo >= hi {
            return Ok(());
        }

        let mut futures = FuturesUnordered::new();
        let mut current_lo = lo;
        let mut total_pruned = 0;

        while current_lo < hi {
            let chunk_hi = (current_lo + T::CHUNK_SIZE).min(hi);
            let semaphore = semaphore.clone();
            let store_clone = store.clone();

            futures.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await?;
                let pool = store_clone.pool();
                let mut conn = pool.get().await?;
                T::prune(current_lo, chunk_hi, &mut conn).await
            }));

            current_lo = chunk_hi;
        }

        while let Some(result) = futures.next().await {
            total_pruned += result??;
        }
    }
}

pub fn spawn_pruner<T: Prunable>(
    cancel: CancellationToken,
    store: PgIndexerStore,
) -> JoinHandle<IndexerResult<()>> {
    spawn_monitored_task!(run_pruner::<T>(cancel, store))
}
