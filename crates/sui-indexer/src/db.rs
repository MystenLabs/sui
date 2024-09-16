// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use clap::Args;
use diesel::migration::{Migration, MigrationSource, MigrationVersion};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_dsl::RunQueryDsl;
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::{Pool, PooledConnection};
use diesel::PgConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use std::time::Duration;
use tracing::info;

table! {
    __diesel_schema_migrations (version) {
        version -> VarChar,
        run_on -> Timestamp,
    }
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/pg");

pub type ConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

#[derive(Args, Debug, Clone)]
pub struct ConnectionPoolConfig {
    #[arg(long, default_value_t = 100)]
    #[arg(env = "DB_POOL_SIZE")]
    pub pool_size: u32,
    #[arg(long, value_parser = parse_duration, default_value = "30")]
    #[arg(env = "DB_CONNECTION_TIMEOUT")]
    pub connection_timeout: Duration,
    #[arg(long, value_parser = parse_duration, default_value = "3600")]
    #[arg(env = "DB_STATEMENT_TIMEOUT")]
    pub statement_timeout: Duration,
}

fn parse_duration(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(std::time::Duration::from_secs(seconds))
}

impl ConnectionPoolConfig {
    const DEFAULT_POOL_SIZE: u32 = 100;
    const DEFAULT_CONNECTION_TIMEOUT: u64 = 30;
    const DEFAULT_STATEMENT_TIMEOUT: u64 = 3600;

    pub(crate) fn connection_config(&self) -> ConnectionConfig {
        ConnectionConfig {
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

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            pool_size: Self::DEFAULT_POOL_SIZE,
            connection_timeout: Duration::from_secs(Self::DEFAULT_CONNECTION_TIMEOUT),
            statement_timeout: Duration::from_secs(Self::DEFAULT_STATEMENT_TIMEOUT),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionConfig {
    pub statement_timeout: Duration,
    pub read_only: bool,
}

impl diesel::r2d2::CustomizeConnection<PgConnection, diesel::r2d2::Error> for ConnectionConfig {
    fn on_acquire(&self, conn: &mut PgConnection) -> std::result::Result<(), diesel::r2d2::Error> {
        diesel::sql_query(format!(
            "SET statement_timeout = {}",
            self.statement_timeout.as_millis(),
        ))
        .execute(conn)
        .map_err(diesel::r2d2::Error::QueryError)?;

        if self.read_only {
            diesel::sql_query("SET default_transaction_read_only = 't'")
                .execute(conn)
                .map_err(diesel::r2d2::Error::QueryError)?;
        }
        Ok(())
    }
}

pub fn new_connection_pool(
    db_url: &str,
    config: &ConnectionPoolConfig,
) -> Result<ConnectionPool, IndexerError> {
    let manager = ConnectionManager::<PgConnection>::new(db_url);

    Pool::builder()
        .max_size(config.pool_size)
        .connection_timeout(config.connection_timeout)
        .connection_customizer(Box::new(config.connection_config()))
        .build(manager)
        .map_err(|e| {
            IndexerError::PgConnectionPoolInitError(format!(
                "Failed to initialize connection pool for {db_url} with error: {e:?}"
            ))
        })
}

pub fn get_pool_connection(pool: &ConnectionPool) -> Result<PoolConnection, IndexerError> {
    pool.get().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

/// Checks that the local migration scripts is a prefix of the records in the database.
/// This allows us run migration scripts against a DB at anytime, without worrying about
/// existing readers fail over.
/// We do however need to make sure that whenever we are deploying a new version of either reader or writer,
/// we must first run migration scripts to ensure that there is not more local scripts than in the DB record.
pub fn check_db_migration_consistency(conn: &mut PoolConnection) -> Result<(), IndexerError> {
    info!("Starting compatibility check");
    let migrations: Vec<Box<dyn Migration<Pg>>> = MIGRATIONS.migrations().map_err(|err| {
        IndexerError::DbMigrationError(format!(
            "Failed to fetch local migrations from schema: {err}"
        ))
    })?;
    let local_migrations: Vec<_> = migrations.iter().map(|m| m.name().version()).collect();
    check_db_migration_consistency_impl(conn, local_migrations)?;
    info!("Compatibility check passed");
    Ok(())
}

fn check_db_migration_consistency_impl(
    conn: &mut PoolConnection,
    local_migrations: Vec<MigrationVersion>,
) -> Result<(), IndexerError> {
    // Unfortunately we cannot call applied_migrations() directly on the connection,
    // since it implicitly creates the __diesel_schema_migrations table if it doesn't exist,
    // which is a write operation that we don't want to do in this function.
    let applied_migrations: Vec<MigrationVersion> = __diesel_schema_migrations::table
        .select(__diesel_schema_migrations::version)
        .order(__diesel_schema_migrations::version.asc())
        .load(conn)?;

    // We check that the local migrations is a prefix of the applied migrations.
    if local_migrations.len() > applied_migrations.len() {
        return Err(IndexerError::DbMigrationError(format!(
            "The number of local migrations is greater than the number of applied migrations. Local migrations: {:?}, Applied migrations: {:?}",
            local_migrations, applied_migrations
        )));
    }
    for (local_migration, applied_migration) in local_migrations.iter().zip(&applied_migrations) {
        if local_migration != applied_migration {
            return Err(IndexerError::DbMigrationError(format!(
                "The next applied migration `{:?}` diverges from the local migration `{:?}`",
                applied_migration, local_migration
            )));
        }
    }
    Ok(())
}

pub use setup_postgres::{reset_database, run_migrations};

pub mod setup_postgres {
    use crate::db::{PoolConnection, MIGRATIONS};
    use anyhow::anyhow;
    use diesel::migration::MigrationConnection;
    use diesel::RunQueryDsl;
    use diesel_migrations::MigrationHarness;
    use tracing::info;

    pub async fn reset_database(conn: &mut PoolConnection) -> Result<(), anyhow::Error> {
        info!("Resetting PG database ...");

        let drop_all_tables = "
        DO $$ DECLARE
            r RECORD;
        BEGIN
        FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public')
            LOOP
                EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
            END LOOP;
        END $$;";
        diesel::sql_query(drop_all_tables).execute(conn)?;
        info!("Dropped all tables.");

        let drop_all_procedures = "
        DO $$ DECLARE
            r RECORD;
        BEGIN
            FOR r IN (SELECT proname, oidvectortypes(proargtypes) as argtypes
                      FROM pg_proc INNER JOIN pg_namespace ns ON (pg_proc.pronamespace = ns.oid)
                      WHERE ns.nspname = 'public' AND prokind = 'p')
            LOOP
                EXECUTE 'DROP PROCEDURE IF EXISTS ' || quote_ident(r.proname) || '(' || r.argtypes || ') CASCADE';
            END LOOP;
        END $$;";
        diesel::sql_query(drop_all_procedures).execute(conn)?;
        info!("Dropped all procedures.");

        let drop_all_functions = "
        DO $$ DECLARE
            r RECORD;
        BEGIN
            FOR r IN (SELECT proname, oidvectortypes(proargtypes) as argtypes
                      FROM pg_proc INNER JOIN pg_namespace ON (pg_proc.pronamespace = pg_namespace.oid)
                      WHERE pg_namespace.nspname = 'public' AND prokind = 'f')
            LOOP
                EXECUTE 'DROP FUNCTION IF EXISTS ' || quote_ident(r.proname) || '(' || r.argtypes || ') CASCADE';
            END LOOP;
        END $$;";
        diesel::sql_query(drop_all_functions).execute(conn)?;
        info!("Dropped all functions.");

        conn.setup()?;
        info!("Created __diesel_schema_migrations table.");

        run_migrations(conn).await?;
        info!("Reset database complete.");
        Ok(())
    }

    pub async fn run_migrations(conn: &mut PoolConnection) -> Result<(), anyhow::Error> {
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
        Ok(())
    }
}

#[cfg(feature = "pg_integration")]
#[cfg(test)]
mod tests {
    use crate::db::{
        check_db_migration_consistency, check_db_migration_consistency_impl, get_pool_connection,
        new_connection_pool, reset_database, ConnectionPoolConfig, MIGRATIONS,
    };
    use crate::tempdb::TempDb;
    use diesel::migration::{Migration, MigrationSource};
    use diesel::pg::Pg;
    use diesel_migrations::MigrationHarness;

    // Check that the migration records in the database created from the local schema
    // pass the consistency check.
    #[tokio::test]
    async fn db_migration_consistency_smoke_test() {
        let database = TempDb::new().unwrap();
        let blocking_pool = new_connection_pool(
            database.database().url().as_str(),
            &ConnectionPoolConfig::default(),
        )
        .unwrap();
        let mut conn = get_pool_connection(&blocking_pool).unwrap();
        reset_database(&mut conn).await.unwrap();
        check_db_migration_consistency(&mut conn).unwrap();
    }

    #[tokio::test]
    async fn db_migration_consistency_non_prefix_test() {
        let database = TempDb::new().unwrap();
        let blocking_pool = new_connection_pool(
            database.database().url().as_str(),
            &ConnectionPoolConfig::default(),
        )
        .unwrap();
        let mut conn = get_pool_connection(&blocking_pool).unwrap();

        reset_database(&mut conn).await.unwrap();

        conn.revert_migration(MIGRATIONS.migrations().unwrap().last().unwrap())
            .unwrap();
        // Local migrations is one record more than the applied migrations.
        // This will fail the consistency check since it's not a prefix.
        assert!(check_db_migration_consistency(&mut conn).is_err());

        conn.run_pending_migrations(MIGRATIONS).unwrap();
        // After running pending migrations they should be consistent.
        check_db_migration_consistency(&mut conn).unwrap();
    }

    #[tokio::test]
    async fn db_migration_consistency_prefix_test() {
        let database = TempDb::new().unwrap();
        let blocking_pool = new_connection_pool(
            database.database().url().as_str(),
            &ConnectionPoolConfig::default(),
        )
        .unwrap();
        let mut conn = get_pool_connection(&blocking_pool).unwrap();
        reset_database(&mut conn).await.unwrap();

        let migrations: Vec<Box<dyn Migration<Pg>>> = MIGRATIONS.migrations().unwrap();
        let mut local_migrations: Vec<_> = migrations.iter().map(|m| m.name().version()).collect();
        local_migrations.pop();
        // Local migrations is one record less than the applied migrations.
        // This should pass the consistency check since it's still a prefix.
        check_db_migration_consistency_impl(&mut conn, local_migrations).unwrap();
    }
}
