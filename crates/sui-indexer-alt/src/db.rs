// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel_async::{
    pooled_connection::{
        bb8::{Pool, PooledConnection, RunError},
        AsyncDieselConnectionManager, PoolError,
    },
    AsyncPgConnection,
};
use url::Url;

#[derive(Clone)]
pub struct Db {
    pool: Pool<AsyncPgConnection>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DbConfig {
    /// The URL of the database to connect to.
    #[arg(long)]
    database_url: Url,

    /// Number of connections to keep in the pool.
    #[arg(long, default_value_t = 100)]
    connection_pool_size: u32,

    /// Time spent waiting for a connection from the pool to become available.
    #[arg(
        long,
        default_value = "60",
        value_name = "SECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_secs)
    )]
    connection_timeout: Duration,
}

pub type Connection<'p> = PooledConnection<'p, AsyncPgConnection>;

impl Db {
    /// Construct a new DB connection pool. Instances of [Db] can be cloned to share access to the
    /// same pool.
    pub async fn new(config: DbConfig) -> Result<Self, PoolError> {
        let manager = AsyncDieselConnectionManager::new(config.database_url.as_str());

        let pool = Pool::builder()
            .max_size(config.connection_pool_size)
            .connection_timeout(config.connection_timeout)
            .build(manager)
            .await?;

        Ok(Self { pool })
    }

    /// Retrieves a connection from the pool. Can fail with a timeout if a connection cannot be
    /// established before the [DbConfig::connection_timeout] has elapsed.
    pub(crate) async fn connect(&self) -> Result<Connection<'_>, RunError> {
        self.pool.get().await
    }

    /// Statistics about the connection pool
    pub(crate) fn state(&self) -> bb8::State {
        self.pool.state()
    }
}
