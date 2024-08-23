// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) use indexer_store::*;
pub use pg_indexer_store::PgIndexerStore;

pub mod indexer_store;
pub mod package_resolver;
mod pg_indexer_store;
pub mod pg_partition_manager;

pub mod diesel_macro {
    thread_local! {
        pub static CALLED_FROM_BLOCKING_POOL: std::cell::RefCell<bool> = const { std::cell::RefCell::new(false) };
    }

    #[macro_export]
    macro_rules! read_only_repeatable_blocking {
        ($pool:expr, $query:expr) => {{
            use downcast::Any;
            use $crate::db::get_pool_connection;
            use $crate::db::PoolConnection;
            #[cfg(feature = "postgres-feature")]
            {
                let mut pool_conn = get_pool_connection($pool)?;
                pool_conn
                    .as_any_mut()
                    .downcast_mut::<PoolConnection<diesel::PgConnection>>()
                    .unwrap()
                    .build_transaction()
                    .read_only()
                    .repeatable_read()
                    .run($query)
                    .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
            }
            #[cfg(feature = "mysql-feature")]
            #[cfg(not(feature = "postgres-feature"))]
            {
                let mut pool_conn = get_pool_connection($pool)?;
                pool_conn
                    .as_any_mut()
                    .downcast_mut::<PoolConnection<diesel::MysqlConnection>>()
                    .unwrap()
                    .transaction($query)
                    .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
            }
        }};
    }

    #[macro_export]
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            use downcast::Any;
            use $crate::db::get_pool_connection;
            use $crate::db::PoolConnection;
            #[cfg(feature = "postgres-feature")]
            {
                let mut pool_conn = get_pool_connection($pool)?;
                pool_conn
                    .as_any_mut()
                    .downcast_mut::<PoolConnection<diesel::PgConnection>>()
                    .unwrap()
                    .build_transaction()
                    .read_only()
                    .run($query)
                    .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
            }
            #[cfg(feature = "mysql-feature")]
            #[cfg(not(feature = "postgres-feature"))]
            {
                use diesel::Connection;
                let mut pool_conn = get_pool_connection($pool)?;
                pool_conn
                    .as_any_mut()
                    .downcast_mut::<PoolConnection<diesel::MysqlConnection>>()
                    .unwrap()
                    .transaction($query)
                    .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
            }
        }};
    }

    #[macro_export]
    macro_rules! transactional_blocking_with_retry {
        ($pool:expr, $query:expr, $max_elapsed:expr) => {{
            use $crate::db::get_pool_connection;
            use $crate::db::PoolConnection;
            use $crate::errors::IndexerError;
            let mut backoff = backoff::ExponentialBackoff::default();
            backoff.max_elapsed_time = Some($max_elapsed);
            let result = match backoff::retry(backoff, || {
                #[cfg(feature = "postgres-feature")]
                {
                    let mut pool_conn =
                        get_pool_connection($pool).map_err(|e| backoff::Error::Transient {
                            err: IndexerError::PostgresWriteError(e.to_string()),
                            retry_after: None,
                        })?;
                    pool_conn
                        .as_any_mut()
                        .downcast_mut::<PoolConnection<diesel::PgConnection>>()
                        .unwrap()
                        .build_transaction()
                        .read_write()
                        .run($query)
                        .map_err(|e| {
                            tracing::error!(
                                "Error with persisting data into DB: {:?}, retrying...",
                                e
                            );
                            backoff::Error::Transient {
                                err: IndexerError::PostgresWriteError(e.to_string()),
                                retry_after: None,
                            }
                        })
                }
                #[cfg(feature = "mysql-feature")]
                #[cfg(not(feature = "postgres-feature"))]
                {
                    use diesel::Connection;
                    let mut pool_conn =
                        get_pool_connection($pool).map_err(|e| backoff::Error::Transient {
                            err: IndexerError::PostgresWriteError(e.to_string()),
                            retry_after: None,
                        })?;
                    pool_conn
                        .as_any_mut()
                        .downcast_mut::<PoolConnection<diesel::MysqlConnection>>()
                        .unwrap()
                        .transaction($query)
                        .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
                        .map_err(|e| {
                            tracing::error!(
                                "Error with persisting data into DB: {:?}, retrying...",
                                e
                            );
                            backoff::Error::Transient {
                                err: IndexerError::PostgresWriteError(e.to_string()),
                                retry_after: None,
                            }
                        })
                }
            }) {
                Ok(v) => Ok(v),
                Err(backoff::Error::Transient { err, .. }) => Err(err),
                Err(backoff::Error::Permanent(err)) => Err(err),
            };
            result
        }};
    }

    #[macro_export]
    macro_rules! spawn_read_only_blocking {
        ($pool:expr, $query:expr, $repeatable_read:expr) => {{
            use downcast::Any;
            use $crate::db::get_pool_connection;
            use $crate::db::PoolConnection;
            use $crate::errors::IndexerError;
            use $crate::store::diesel_macro::CALLED_FROM_BLOCKING_POOL;
            let current_span = tracing::Span::current();
            tokio::task::spawn_blocking(move || {
                CALLED_FROM_BLOCKING_POOL
                    .with(|in_blocking_pool| *in_blocking_pool.borrow_mut() = true);
                let _guard = current_span.enter();
                let mut pool_conn = get_pool_connection($pool).unwrap();
                #[cfg(feature = "postgres-feature")]
                {
                    if $repeatable_read {
                        pool_conn
                            .as_any_mut()
                            .downcast_mut::<PoolConnection<diesel::PgConnection>>()
                            .unwrap()
                            .build_transaction()
                            .read_only()
                            .repeatable_read()
                            .run($query)
                            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
                    } else {
                        pool_conn
                            .as_any_mut()
                            .downcast_mut::<PoolConnection<diesel::PgConnection>>()
                            .unwrap()
                            .build_transaction()
                            .read_only()
                            .run($query)
                            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
                    }
                }
                #[cfg(feature = "mysql-feature")]
                #[cfg(not(feature = "postgres-feature"))]
                {
                    use diesel::Connection;
                    pool_conn
                        .as_any_mut()
                        .downcast_mut::<PoolConnection<diesel::MysqlConnection>>()
                        .unwrap()
                        .transaction($query)
                        .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
                }
            })
            .await
            .expect("Blocking call failed")
        }};
    }

    #[macro_export]
    macro_rules! insert_or_ignore_into {
        ($table:expr, $values:expr, $conn:expr) => {{
            use diesel::RunQueryDsl;
            let error_message = concat!("Failed to write to ", stringify!($table), " DB");
            #[cfg(feature = "postgres-feature")]
            {
                diesel::insert_into($table)
                    .values($values)
                    .on_conflict_do_nothing()
                    .execute($conn)
                    .map_err(IndexerError::from)
                    .context(error_message)?;
            }
            #[cfg(feature = "mysql-feature")]
            #[cfg(not(feature = "postgres-feature"))]
            {
                diesel::insert_or_ignore_into($table)
                    .values($values)
                    .execute($conn)
                    .map_err(IndexerError::from)
                    .context(error_message)?;
            }
        }};
    }

    #[macro_export]
    macro_rules! on_conflict_do_update {
        ($table:expr, $values:expr, $target:expr, $pg_columns:expr, $mysql_columns:expr, $conn:expr) => {{
            use diesel::ExpressionMethods;
            use diesel::RunQueryDsl;
            #[cfg(feature = "postgres-feature")]
            {
                diesel::insert_into($table)
                    .values($values)
                    .on_conflict($target)
                    .do_update()
                    .set($pg_columns)
                    .execute($conn)?;
            }
            #[cfg(feature = "mysql-feature")]
            #[cfg(not(feature = "postgres-feature"))]
            {
                for excluded_row in $values.iter() {
                    let columns = $mysql_columns;
                    diesel::insert_into($table)
                        .values(excluded_row.clone())
                        .on_conflict(diesel::dsl::DuplicatedKeys)
                        .do_update()
                        .set(columns(excluded_row.clone()))
                        .execute($conn)?;
                }
            }
        }};
    }

    #[macro_export]
    macro_rules! run_query {
        ($pool:expr, $query:expr) => {{
            blocking_call_is_ok_or_panic!();
            read_only_blocking!($pool, $query)
        }};
    }

    #[macro_export]
    macro_rules! run_query_repeatable {
        ($pool:expr, $query:expr) => {{
            blocking_call_is_ok_or_panic!();
            read_only_repeatable_blocking!($pool, $query)
        }};
    }

    #[macro_export]
    macro_rules! run_query_async {
        ($pool:expr, $query:expr) => {{
            spawn_read_only_blocking!($pool, $query, false)
        }};
    }

    #[macro_export]
    macro_rules! run_query_repeatable_async {
        ($pool:expr, $query:expr) => {{
            spawn_read_only_blocking!($pool, $query, true)
        }};
    }

    /// Check that we are in a context conducive to making blocking calls.
    /// This is done by either:
    /// - Checking that we are not inside a tokio runtime context
    ///
    /// Or:
    /// - If we are inside a tokio runtime context, ensure that the call went through
    ///     `IndexerReader::spawn_blocking` which properly moves the blocking call to a blocking thread
    ///     pool.
    #[macro_export]
    macro_rules! blocking_call_is_ok_or_panic {
        () => {{
            use $crate::store::diesel_macro::CALLED_FROM_BLOCKING_POOL;
            if tokio::runtime::Handle::try_current().is_ok()
                && !CALLED_FROM_BLOCKING_POOL.with(|in_blocking_pool| *in_blocking_pool.borrow())
            {
                panic!(
                    "You are calling a blocking DB operation directly on an async thread. \
                        Please use IndexerReader::spawn_blocking instead to move the \
                        operation to a blocking thread"
                );
            }
        }};
    }

    #[macro_export]
    macro_rules! persist_chunk_into_table {
        ($table:expr, $chunk:expr, $pool:expr) => {{
            let now = std::time::Instant::now();
            let chunk_len = $chunk.len();
            transactional_blocking_with_retry!(
                $pool,
                |conn| {
                    for chunk in $chunk.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        insert_or_ignore_into!($table, chunk, conn);
                    }
                    Ok::<(), IndexerError>(())
                },
                PG_DB_COMMIT_SLEEP_DURATION
            )
            .tap_ok(|_| {
                let elapsed = now.elapsed().as_secs_f64();
                info!(
                    elapsed,
                    "Persisted {} rows to {}",
                    chunk_len,
                    stringify!($table),
                );
            })
            .tap_err(|e| {
                tracing::error!("Failed to persist {} with error: {}", stringify!($table), e);
            })
        }};
    }

    pub use blocking_call_is_ok_or_panic;
    pub use read_only_blocking;
    pub use read_only_repeatable_blocking;
    pub use run_query;
    pub use run_query_async;
    pub use run_query_repeatable;
    pub use run_query_repeatable_async;
    pub use spawn_read_only_blocking;
    pub use transactional_blocking_with_retry;
}
