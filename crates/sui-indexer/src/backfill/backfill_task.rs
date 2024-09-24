// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::ConnectionPool;
use async_trait::async_trait;
use std::ops::RangeInclusive;

/// Trait for performing backfill tasks: querying and committing data.
#[async_trait]
pub trait BackfillTask<T: Send>: Send + Sync {
    /// Queries the database for a specific range.
    async fn query_db(pool: ConnectionPool, range: &RangeInclusive<usize>) -> T;

    /// Batch commits the processed results back to the database.
    async fn commit_db(pool: ConnectionPool, results: ProcessedResult<T>);
}

pub struct ProcessedResult<T> {
    pub output: T,
    pub range: RangeInclusive<usize>,
}
