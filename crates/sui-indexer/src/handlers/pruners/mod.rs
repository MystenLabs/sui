// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::Connection;

pub mod objects_history;

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
pub trait Pruner {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: &'static str;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// If at least this many rows are pending, the committer will commit them eagerly.
    const BATCH_SIZE: usize = 50;

    /// How many rows to delete at once.
    const CHUNK_SIZE: usize = 100000;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_SIZE: usize = 1000;

    /// Earliest available data in the table.
    async fn data_lo(conn: &mut Connection<'_>) -> anyhow::Result<u64>;

    /// Prune the table between `[prune_lo, prune_hi)`.
    async fn prune(
        prune_lo: u64,
        prune_hi: u64,
        conn: &mut Connection<'_>,
    ) -> anyhow::Result<usize>;
}
