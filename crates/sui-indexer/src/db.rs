// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::max;
use std::time::Duration;

use crate::errors::IndexerError;
use crate::{run_query_async, spawn_read_only_blocking};
use clap::Args;
use diesel::migration::{Migration, MigrationSource};
use diesel::pg::Pg;
use diesel::query_dsl::RunQueryDsl;
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::{Pool, PooledConnection};
use diesel::PgConnection;
use diesel::{sql_query, QueryableByName};
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use tracing::info;

pub type ConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/pg");

const LAST_MIGRATION_IN_V1: &str = "2023-11-29-193859_advance_partition";
const LAST_MIGRATION_IN_V2: &str = "2024-07-13-003534_chain_identifier";

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

    fn connection_config(&self) -> ConnectionConfig {
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

pub fn reset_database(conn: &mut PoolConnection) -> Result<(), anyhow::Error> {
    setup_postgres::reset_database(conn)?;
    Ok(())
}

#[derive(QueryableByName)]
pub struct StoredMigration {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub version: String,
}

pub async fn check_db_migration_consistency(pool: ConnectionPool) -> Result<(), IndexerError> {
    info!("Starting compatibility check");
    let query = "SELECT version FROM __diesel_schema_migrations ORDER BY version";
    let mut db_migrations: Vec<_> = run_query_async!(&pool, move |conn| {
        sql_query(query).load::<StoredMigration>(conn)
    })?
    .into_iter()
    .map(|m| m.version)
    .collect();
    db_migrations.sort();
    info!("Migration Records from the DB: {:?}", db_migrations);

    let known_migrations = get_all_local_migrations()
        .into_iter()
        .map(|m| {
            let full_name = m.name().to_string();
            let processed = full_name.replace("-", "");
            let first_non_digit = processed
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(processed.len());
            processed[..first_non_digit].to_string()
        })
        .collect::<Vec<_>>();
    info!(
        "Migration Records from local schema: {:?}",
        known_migrations
    );
    for i in 0..max(known_migrations.len(), db_migrations.len()) {
        let local_migration_record = known_migrations.get(i).cloned().unwrap_or_default();
        let db_migration_record = db_migrations.get(i).cloned().unwrap_or_default();
        if known_migrations.get(i) != db_migrations.get(i) {
            return Err(IndexerError::DbMigrationRecordMismatch {
                local_migration_record,
                db_migration_record,
            });
        }
    }
    info!("Compatibility check passed");
    Ok(())
}

fn get_all_local_migrations() -> Vec<Box<dyn Migration<Pg>>> {
    let mut known_migrations: Vec<_> = MIGRATIONS.migrations().unwrap();
    // We expect the DB migration record to end at the last migration
    // known to the build based on the release.
    let last_migration_pos = if cfg!(feature = "schema_v3") {
        known_migrations.len() - 1
    } else if cfg!(feature = "schema_v2") {
        known_migrations
            .iter()
            .position(|m| m.name().to_string() == LAST_MIGRATION_IN_V2)
            .expect("Last migration not found in known migrations")
    } else {
        assert!(cfg!(feature = "schema_v1"));
        // production schema.
        known_migrations
            .iter()
            .position(|m| m.name().to_string() == LAST_MIGRATION_IN_V1)
            .expect("Last migration not found in known migrations")
    };
    known_migrations.truncate(last_migration_pos + 1);
    known_migrations
}

pub mod setup_postgres {
    use crate::db::{get_all_local_migrations, PoolConnection};
    use anyhow::anyhow;
    use diesel::RunQueryDsl;
    use diesel_migrations::MigrationHarness;
    use tracing::info;

    pub fn reset_database(conn: &mut PoolConnection) -> Result<(), anyhow::Error> {
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

        diesel::sql_query(
            "
        CREATE TABLE IF NOT EXISTS __diesel_schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            run_on TIMESTAMP NOT NULL DEFAULT NOW()
        )",
        )
        .execute(conn)?;
        info!("Created __diesel_schema_migrations table.");

        conn.run_migrations(&get_all_local_migrations())
            .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
        info!("Reset database complete.");
        Ok(())
    }
}

#[cfg(feature = "pg_integration")]
#[cfg(test)]
mod tests {
    use crate::db::{
        check_db_migration_consistency, new_connection_pool, setup_postgres::reset_database,
        ConnectionPoolConfig,
    };

    #[tokio::test]
    async fn test_db_consistency_check() {
        let db_url = "postgres://postgres:postgrespw@localhost:5432/sui_indexer";
        let config = ConnectionPoolConfig::default();
        let pool = new_connection_pool(db_url, &config).unwrap();
        reset_database(&mut pool.get().unwrap()).unwrap();
        check_db_migration_consistency(pool).await.unwrap();
    }
}
