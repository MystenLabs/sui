// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use diesel::prelude::ConnectionError;
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::bb8::PooledConnection;
use diesel_async::pooled_connection::bb8::RunError;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::PoolError;
use diesel_async::RunQueryDsl;
use diesel_async::{AsyncConnection, AsyncPgConnection};
use futures::FutureExt;
use url::Url;

use crate::db::ConnectionConfig;
use crate::db::ConnectionPoolConfig;

#[derive(Clone, Debug)]
pub struct ConnectionPool {
    database_url: Arc<Url>,
    pool: Pool<AsyncPgConnection>,
}

impl ConnectionPool {
    pub async fn new(database_url: Url, config: ConnectionPoolConfig) -> Result<Self, PoolError> {
        let database_url = Arc::new(database_url);
        let connection_config = config.connection_config();
        let mut manager_config = diesel_async::pooled_connection::ManagerConfig::default();
        manager_config.custom_setup =
            Box::new(move |url| establish_connection(url, connection_config).boxed());
        let manager =
            AsyncDieselConnectionManager::new_with_config(database_url.as_str(), manager_config);

        Pool::builder()
            .max_size(config.pool_size)
            .connection_timeout(config.connection_timeout)
            .build(manager)
            .await
            .map(|pool| Self { database_url, pool })
    }

    /// Retrieves a connection from the pool.
    pub async fn get(&self) -> Result<Connection<'_>, RunError> {
        self.pool.get().await.map(Connection::PooledConnection)
    }

    /// Get a new dedicated connection that will not be managed by the pool.
    /// An application may want a persistent connection (e.g. to do a
    /// postgres LISTEN) that will not be closed or repurposed by the pool.
    ///
    /// This method allows reusing the manager's configuration but otherwise
    /// bypassing the pool
    pub async fn dedicated_connection(&self) -> Result<Connection<'static>, PoolError> {
        self.pool
            .dedicated_connection()
            .await
            .map(Connection::Dedicated)
    }

    /// Returns information about the current state of the pool.
    pub fn state(&self) -> bb8::State {
        self.pool.state()
    }

    /// Returns the database url that this pool is configured with
    pub fn url(&self) -> &Url {
        &self.database_url
    }
}

pub enum Connection<'a> {
    PooledConnection(PooledConnection<'a, AsyncPgConnection>),
    Dedicated(AsyncPgConnection),
}

impl Connection<'static> {
    pub async fn dedicated(database_url: &Url) -> Result<Self, ConnectionError> {
        AsyncPgConnection::establish(database_url.as_str())
            .await
            .map(Connection::Dedicated)
    }

    /// Run the provided Migrations
    pub async fn run_pending_migrations<M>(
        self,
        migrations: M,
    ) -> diesel::migration::Result<Vec<diesel::migration::MigrationVersion<'static>>>
    where
        M: diesel::migration::MigrationSource<diesel::pg::Pg> + Send + 'static,
    {
        use diesel::migration::MigrationVersion;
        use diesel_migrations::MigrationHarness;

        let mut connection =
            diesel_async::async_connection_wrapper::AsyncConnectionWrapper::<Self>::from(self);

        tokio::task::spawn_blocking(move || {
            connection
                .run_pending_migrations(migrations)
                .map(|versions| versions.iter().map(MigrationVersion::as_owned).collect())
        })
        .await
        .unwrap()
    }
}

impl<'a> std::ops::Deref for Connection<'a> {
    type Target = AsyncPgConnection;

    fn deref(&self) -> &Self::Target {
        match self {
            Connection::PooledConnection(pooled) => pooled.deref(),
            Connection::Dedicated(dedicated) => dedicated,
        }
    }
}

impl<'a> std::ops::DerefMut for Connection<'a> {
    fn deref_mut(&mut self) -> &mut AsyncPgConnection {
        match self {
            Connection::PooledConnection(pooled) => pooled.deref_mut(),
            Connection::Dedicated(dedicated) => dedicated,
        }
    }
}

impl ConnectionConfig {
    async fn apply(&self, connection: &mut AsyncPgConnection) -> Result<(), diesel::result::Error> {
        diesel::sql_query(format!(
            "SET statement_timeout = {}",
            self.statement_timeout.as_millis(),
        ))
        .execute(connection)
        .await?;

        if self.read_only {
            diesel::sql_query("SET default_transaction_read_only = 'on'")
                .execute(connection)
                .await?;
        }

        Ok(())
    }
}

/// Function used by the Connection Pool Manager to establish and setup new connections
async fn establish_connection(
    url: &str,
    config: ConnectionConfig,
) -> Result<AsyncPgConnection, ConnectionError> {
    let mut connection = AsyncPgConnection::establish(url).await?;

    config
        .apply(&mut connection)
        .await
        .map_err(ConnectionError::CouldntSetupConfiguration)?;

    Ok(connection)
}
