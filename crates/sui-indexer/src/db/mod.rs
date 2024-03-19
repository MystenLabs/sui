// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod postgres;

use std::any::Any;
use std::time::Duration;
use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{Connection, PgConnection, RunQueryDsl};
use diesel::backend::DieselReserveSpecialization;
use diesel::r2d2::{ConnectionManager, R2D2Connection};
use diesel::result::Error;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use thiserror::Error;
use tracing::info;
use crate::errors::IndexerError;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Failed to connect to database")]
    ConnectionError(diesel::ConnectionError),
    #[error("Database URL environment variable not set")]
    MissingUrlError,
    #[error("Unsupported database backend selected")]
    UnsupportedBackend,
}

trait DBConn {
    fn as_any(&self) -> &dyn Any;
}

// impl<M> DBConn for PooledConnection<M> {
//     fn as_any(&self) -> &dyn Any {
//         self
//     }
// }

pub type ConnectionPool<T> = diesel::r2d2::Pool<ConnectionManager<T>>;
pub type PooledConnection<T> = diesel::r2d2::PooledConnection<ConnectionManager<T>>;

pub fn new_connection_pool<T: R2D2Connection  + 'static>(db_url: &str, pool_size: Option<u32>) -> Result<ConnectionPool<T>, IndexerError>  {
    let manager = ConnectionManager::<T>::new(db_url);
    let pool_size = pool_size.unwrap_or(100);
    diesel::r2d2::Pool::builder()
        .max_size(pool_size)
        .connection_timeout(Duration::from_secs(30))
        .build(manager)
        .map_err(|e| {
            IndexerError::PgConnectionPoolInitError(format!(
                "Failed to initialize connection pool with error: {:?}",
                e
            ))
        })
}

pub fn get_pool_connection<T: R2D2Connection + Send + 'static>(pool: &ConnectionPool<T>) -> Result<PooledConnection<T>, IndexerError> {
    pool.get().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}