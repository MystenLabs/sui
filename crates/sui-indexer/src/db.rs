// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{r2d2::ConnectionManager, PgConnection, RunQueryDsl};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;

use crate::errors::IndexerError;

pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;

#[derive(Debug, Clone, Copy)]
pub struct PgConnectionPoolConfig {
    pub pool_size: u32,
    pub connection_timeout: Duration,
    pub statement_timeout: Duration,
}

impl PgConnectionPoolConfig {
    const DEFAULT_POOL_SIZE: u32 = 100;
    const DEFAULT_CONNECTION_TIMEOUT: u64 = 30;
    const DEFAULT_STATEMENT_TIMEOUT: u64 = 30;

    fn connection_config(&self) -> PgConnectionConfig {
        PgConnectionConfig {
            statement_timeout: self.statement_timeout,
            read_only: false,
        }
    }

    pub fn set_pool_size(&mut self, size: u32) {
        self.pool_size = size;
    }

    pub fn set_connection_timeout(&mut self, timeout: Duration) {
        self.connection_timeout = timeout;
    }

    pub fn set_statement_timeout(&mut self, timeout: Duration) {
        self.statement_timeout = timeout;
    }
}

impl Default for PgConnectionPoolConfig {
    fn default() -> Self {
        let db_pool_size = std::env::var("DB_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(Self::DEFAULT_POOL_SIZE);
        let conn_timeout_secs = std::env::var("DB_CONNECTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_CONNECTION_TIMEOUT);
        let statement_timeout_secs = std::env::var("DB_STATEMENT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_STATEMENT_TIMEOUT);

        Self {
            pool_size: db_pool_size,
            connection_timeout: Duration::from_secs(conn_timeout_secs),
            statement_timeout: Duration::from_secs(statement_timeout_secs),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PgConnectionConfig {
    pub statement_timeout: Duration,
    pub read_only: bool,
}

impl diesel::r2d2::CustomizeConnection<PgConnection, diesel::r2d2::Error> for PgConnectionConfig {
    fn on_acquire(&self, conn: &mut PgConnection) -> std::result::Result<(), diesel::r2d2::Error> {
        use diesel::sql_query;

        sql_query(format!(
            "SET statement_timeout = {}",
            self.statement_timeout.as_millis(),
        ))
        .execute(conn)
        .map_err(diesel::r2d2::Error::QueryError)?;

        if self.read_only {
            sql_query("SET default_transaction_read_only = 't'")
                .execute(conn)
                .map_err(diesel::r2d2::Error::QueryError)?;
        }

        Ok(())
    }
}

pub fn new_pg_connection_pool(
    db_url: &str,
    pool_size: Option<u32>,
) -> Result<PgConnectionPool, IndexerError> {
    let pool_config = PgConnectionPoolConfig::default();
    let manager = ConnectionManager::<PgConnection>::new(db_url);

    let pool_size = pool_size.unwrap_or(pool_config.pool_size);
    diesel::r2d2::Pool::builder()
        .max_size(pool_size)
        .connection_timeout(pool_config.connection_timeout)
        .connection_customizer(Box::new(pool_config.connection_config()))
        .build(manager)
        .map_err(|e| {
            IndexerError::PgConnectionPoolInitError(format!(
                "Failed to initialize connection pool with error: {:?}",
                e
            ))
        })
}

pub fn get_pg_pool_connection(pool: &PgConnectionPool) -> Result<PgPoolConnection, IndexerError> {
    pool.get().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Resets the database by reverting all migrations and reapplying them.
///
/// If `drop_all` is set to `true`, the function will drop all tables in the database before
/// resetting the migrations. This option is destructive and will result in the loss of all
/// data in the tables. Use with caution, especially in production environments.
pub fn reset_database(conn: &mut PgPoolConnection, drop_all: bool) -> Result<(), anyhow::Error> {
    info!("Resetting database ...");
    if drop_all {
        drop_all_tables(conn)
            .map_err(|e| anyhow!("Encountering error when dropping all tables {e}"))?;
    } else {
        conn.revert_all_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("Error reverting all migrations {e}"))?;
    }
    conn.run_migrations(&MIGRATIONS.migrations().unwrap())
        .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
    info!("Reset database complete.");
    Ok(())
}

fn drop_all_tables(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
    info!("Dropping all tables in the database");
    let table_names: Vec<String> = diesel::dsl::sql::<diesel::sql_types::Text>(
        "
        SELECT tablename FROM pg_tables WHERE schemaname = 'public'
    ",
    )
    .load(conn)?;

    for table_name in table_names {
        let drop_table_query = format!("DROP TABLE IF EXISTS {} CASCADE", table_name);
        diesel::sql_query(drop_table_query).execute(conn)?;
    }

    // Recreate the __diesel_schema_migrations table
    diesel::sql_query(
        "
        CREATE TABLE __diesel_schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            run_on TIMESTAMP NOT NULL DEFAULT NOW()
        )
    ",
    )
    .execute(conn)?;
    info!("Dropped all tables in the database");
    Ok(())
}
