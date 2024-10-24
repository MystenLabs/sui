// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;

use crate::db;

pub mod ev_emit_mod;
pub mod ev_struct_inst;
pub mod kv_checkpoints;
pub mod kv_objects;
pub mod kv_transactions;
pub mod tx_affected_addresses;
pub mod tx_affected_objects;
pub mod tx_balance_changes;
pub mod tx_calls_fun;
pub mod tx_digests;
pub mod tx_kinds;

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data into
/// rows for their table, and how to write those rows to the database.
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
pub trait Handler {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: &'static str;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// If at least this many rows are pending, the committer will commit them eagerly.
    const BATCH_SIZE: usize = 50;

    /// If there are more than this many rows pending, the committer will only commit this many in
    /// one operation.
    const CHUNK_SIZE: usize = 200;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_SIZE: usize = 1000;

    /// The type of value being inserted by the handler.
    type Value: Send + Sync + 'static;

    /// The processing logic for turning a checkpoint into rows of the table.
    fn process(checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>>;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>)
        -> anyhow::Result<usize>;
}
