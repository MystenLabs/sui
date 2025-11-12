// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Postgres-specific handler trait for concurrent indexing pipelines.
//!
//! This module provides an interface for handlers that need to respect
//! PostgreSQL's bind parameter limit (32,767 parameters per query). When inserting multiple rows,
//! each field becomes a bind parameter, so the maximum number of rows per batch is:
//!
//! ```text
//! max_rows = 32,767 / fields_per_row
//! ```
//!
//! The `Handler` trait in this module extends the framework's concurrent Handler trait with
//! Postgres-specific batching logic that automatically handles this limitation.
//!
//! # TODO: Consider moving FieldCount to sui-pg-db
//!
//! Currently, FieldCount lives in this framework crate but is Postgres-specific. Ideally it should
//! live in sui-pg-db. However, this creates a circular dependency:
//! - sui-indexer-alt-framework depends on sui-pg-db (for IndexerCluster and other utilities)
//! - This blanket impl needs FieldCount to implement concurrent::Handler for postgres indexers
//! - Moving FieldCount to sui-pg-db would require framework to depend on sui-pg-db (circular!)
//!
//! To fully decouple, we'd need to move all postgres-specific code (including IndexerCluster) to
//! sui-pg-db, which would be a much larger breaking change. Consider this for a future refactor.
//!
//! See: <https://github.com/MystenLabs/sui/pull/24055#issuecomment-3471278182>

use async_trait::async_trait;

use super::{Connection, Db, FieldCount};
use crate::pipeline::{Processor, concurrent};

/// Postgres-specific handler trait for concurrent indexing pipelines.
///
/// The trait automatically implements the framework's Handler trait with a PgBatch that respects
/// the 32,767 bind parameter limit.
#[async_trait]
pub trait Handler: Processor<Value: FieldCount> {
    /// If at least this many rows are pending, the committer will commit them eagerly.
    const MIN_EAGER_ROWS: usize = 50;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_ROWS: usize = 5000;

    /// The maximum number of watermarks that can show up in a single batch.
    const MAX_WATERMARK_UPDATES: usize = 10_000;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>)
    -> anyhow::Result<usize>;

    /// Clean up data between checkpoints `_from` and `_to_exclusive` (exclusive) in the database,
    /// returning the number of rows affected. This function is optional, and defaults to not
    /// pruning at all.
    async fn prune<'a>(
        &self,
        _from: u64,
        _to_exclusive: u64,
        _conn: &mut Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(0)
    }
}

/// Calculate the maximum number of rows that can be inserted in a single batch,
/// given the number of fields per row.
const fn max_chunk_rows<T: FieldCount>() -> usize {
    if T::FIELD_COUNT == 0 {
        i16::MAX as usize
    } else {
        i16::MAX as usize / T::FIELD_COUNT
    }
}

/// Blanket implementation of the framework's Handler trait for any type implementing the
/// Postgres-specific Handler trait.
#[async_trait]
impl<H> concurrent::Handler for H
where
    H: Handler + Send + Sync + 'static,
    H::Value: FieldCount + Send + Sync,
{
    type Store = Db;
    type Batch = Vec<H::Value>;

    const MIN_EAGER_ROWS: usize = H::MIN_EAGER_ROWS;
    const MAX_PENDING_ROWS: usize = H::MAX_PENDING_ROWS;
    const MAX_WATERMARK_UPDATES: usize = H::MAX_WATERMARK_UPDATES;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> crate::pipeline::concurrent::BatchStatus {
        let max_chunk_rows = max_chunk_rows::<H::Value>();
        let current_len = batch.len();

        if current_len + values.len() > max_chunk_rows {
            // Batch would exceed the limit, take only what fits
            let remaining_capacity = max_chunk_rows - current_len;
            batch.extend(values.take(remaining_capacity));
            crate::pipeline::concurrent::BatchStatus::Ready
        } else {
            // All values fit, take them all
            batch.extend(values);
            crate::pipeline::concurrent::BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as crate::store::Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        H::commit(batch, conn).await
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut <Self::Store as crate::store::Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        <H as Handler>::prune(self, from, to_exclusive, conn).await
    }
}
