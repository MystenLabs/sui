// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel_async::{scoped_futures::ScopedBoxFuture, AsyncPgConnection};
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
    let backoff = backoff::ExponentialBackoff {
        max_elapsed_time: Some(timeout),
        ..Default::default()
    };
    backoff::future::retry(backoff, || async {
        let mut connection = pool.get().await.map_err(|e| backoff::Error::Transient {
            err: IndexerError::PostgresWriteError(e.to_string()),
            retry_after: None,
        })?;

        connection
            .build_transaction()
            .read_write()
            .run(query.clone())
            .await
            .map_err(|e| {
                tracing::error!("Error with persisting data into DB: {:?}, retrying...", e);
                backoff::Error::Transient {
                    err: IndexerError::PostgresWriteError(e.to_string()),
                    retry_after: None,
                }
            })
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
    let backoff = backoff::ExponentialBackoff {
        max_elapsed_time: Some(timeout),
        ..Default::default()
    };
    backoff::future::retry(backoff, || async {
        let mut connection = pool.get().await.map_err(|e| backoff::Error::Transient {
            err: IndexerError::PostgresWriteError(e.to_string()),
            retry_after: None,
        })?;

        connection
            .build_transaction()
            .read_only()
            .run(query.clone())
            .await
            .map_err(|e| {
                tracing::error!("Error with reading data from DB: {:?}, retrying...", e);
                backoff::Error::Transient {
                    err: IndexerError::PostgresWriteError(e.to_string()),
                    retry_after: None,
                }
            })
    })
    .await
}
