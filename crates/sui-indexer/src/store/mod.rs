// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use diesel_async::{AsyncPgConnection, scoped_futures::ScopedBoxFuture};
pub(crate) use indexer_store::*;
pub use pg_indexer_store::PgIndexerStore;

use crate::{database::ConnectionPool, errors::IndexerError};

pub mod indexer_store;
pub mod package_resolver;
mod pg_indexer_store;
pub mod pg_partition_manager;

pub async fn transaction_with_retry<'a, Q, T>(
    pool: &ConnectionPool,
    timeout: Duration,
    query: Q,
) -> Result<T, IndexerError>
where
    Q: for<'r> FnOnce(
            &'r mut AsyncPgConnection,
        ) -> ScopedBoxFuture<'a, 'r, Result<T, IndexerError>>
        + Send,
    Q: Clone,
    T: 'a,
{
    let transaction_fn = || async {
        let mut connection = pool.get().await?;

        connection
            .build_transaction()
            .read_write()
            .run(query.clone())
            .await
    };

    transaction_fn
        .retry(ExponentialBuilder::default().with_max_delay(timeout))
        .when(|e: &IndexerError| {
            tracing::error!("Error with persisting data into DB: {:?}, retrying...", e);
            true
        })
        .await
}

pub async fn read_with_retry<'a, Q, T>(
    pool: &ConnectionPool,
    timeout: Duration,
    query: Q,
) -> Result<T, IndexerError>
where
    Q: for<'r> FnOnce(
            &'r mut AsyncPgConnection,
        ) -> ScopedBoxFuture<'a, 'r, Result<T, IndexerError>>
        + Send,
    Q: Clone,
    T: 'a,
{
    let read_fn = || async {
        let mut connection = pool.get().await?;

        connection
            .build_transaction()
            .read_only()
            .run(query.clone())
            .await
    };

    read_fn
        .retry(ExponentialBuilder::default().with_max_delay(timeout))
        .when(|e: &IndexerError| {
            tracing::error!("Error with reading data from DB: {:?}, retrying...", e);
            true
        })
        .await
}
