// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::Connection;
use crate::errors::IndexerError;
use crate::handlers::pruner::PrunableTable;
use clap::Args;
use diesel::migration::{Migration, MigrationSource, MigrationVersion};
use diesel::pg::Pg;
use diesel::prelude::QueryableByName;
use diesel::table;
use diesel::QueryDsl;
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;
use strum::IntoEnumIterator;
use tracing::info;

table! {
    __diesel_schema_migrations (version) {
        version -> VarChar,
        run_on -> Timestamp,
    }
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/pg");

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

/// Checks that the local migration scripts is a prefix of the records in the database.
/// This allows us run migration scripts against a DB at anytime, without worrying about
/// existing readers fail over.
/// We do however need to make sure that whenever we are deploying a new version of either reader or writer,
/// we must first run migration scripts to ensure that there is not more local scripts than in the DB record.
pub async fn check_db_migration_consistency(conn: &mut Connection<'_>) -> Result<(), IndexerError> {
    info!("Starting compatibility check");
    let migrations: Vec<Box<dyn Migration<Pg>>> = MIGRATIONS.migrations().map_err(|err| {
        IndexerError::DbMigrationError(format!(
            "Failed to fetch local migrations from schema: {err}"
        ))
    })?;
    let local_migrations: Vec<_> = migrations
        .into_iter()
        .map(|m| m.name().version().as_owned())
        .collect();
    check_db_migration_consistency_impl(conn, local_migrations).await?;
    info!("Compatibility check passed");
    Ok(())
}

async fn check_db_migration_consistency_impl(
    conn: &mut Connection<'_>,
    local_migrations: Vec<MigrationVersion<'_>>,
) -> Result<(), IndexerError> {
    use diesel_async::RunQueryDsl;

    // Unfortunately we cannot call applied_migrations() directly on the connection,
    // since it implicitly creates the __diesel_schema_migrations table if it doesn't exist,
    // which is a write operation that we don't want to do in this function.
    let applied_migrations: BTreeSet<MigrationVersion<'_>> = BTreeSet::from_iter(
        __diesel_schema_migrations::table
            .select(__diesel_schema_migrations::version)
            .load(conn)
            .await?,
    );

    // We check that the local migrations is a subset of the applied migrations.
    let unapplied_migrations: Vec<_> = local_migrations
        .into_iter()
        .filter(|m| !applied_migrations.contains(m))
        .collect();

    if unapplied_migrations.is_empty() {
        return Ok(());
    }

    Err(IndexerError::DbMigrationError(format!(
        "This binary expected the following migrations to have been run, and they were not: {:?}",
        unapplied_migrations
    )))
}

/// Check that prunable tables exist in the database.
pub async fn check_prunable_tables_valid(conn: &mut Connection<'_>) -> Result<(), IndexerError> {
    info!("Starting compatibility check");

    use diesel_async::RunQueryDsl;

    let select_parent_tables = r#"
    SELECT c.relname AS table_name
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    LEFT JOIN pg_partitioned_table pt ON pt.partrelid = c.oid
    WHERE c.relkind IN ('r', 'p')  -- 'r' for regular tables, 'p' for partitioned tables
        AND n.nspname = 'public'
        AND (
            pt.partrelid IS NOT NULL  -- This is a partitioned (parent) table
            OR NOT EXISTS (  -- This is not a partition (child table)
                SELECT 1
                FROM pg_inherits i
                WHERE i.inhrelid = c.oid
            )
        );
    "#;

    #[derive(QueryableByName)]
    struct TableName {
        #[diesel(sql_type = diesel::sql_types::Text)]
        table_name: String,
    }

    let result: Vec<TableName> = diesel::sql_query(select_parent_tables)
        .load(conn)
        .await
        .map_err(|e| IndexerError::DbMigrationError(format!("Failed to fetch tables: {e}")))?;

    let parent_tables_from_db: HashSet<_> = result.into_iter().map(|t| t.table_name).collect();

    for key in PrunableTable::iter() {
        if !parent_tables_from_db.contains(key.as_ref()) {
            return Err(IndexerError::GenericError(format!(
                "Invalid retention policy override provided for table {}: does not exist in the database",
                key
            )));
        }
    }

    info!("Compatibility check passed");
    Ok(())
}

pub use setup_postgres::{reset_database, run_migrations};

pub mod setup_postgres {
    use crate::{database::Connection, db::MIGRATIONS};
    use anyhow::anyhow;
    use diesel_async::RunQueryDsl;
    use tracing::info;

    pub async fn reset_database(mut conn: Connection<'static>) -> Result<(), anyhow::Error> {
        info!("Resetting PG database ...");
        clear_database(&mut conn).await?;
        run_migrations(conn).await?;
        info!("Reset database complete.");
        Ok(())
    }

    pub async fn clear_database(conn: &mut Connection<'static>) -> Result<(), anyhow::Error> {
        info!("Clearing the database...");
        let drop_all_tables = "
        DO $$ DECLARE
            r RECORD;
        BEGIN
        FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public')
            LOOP
                EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
            END LOOP;
        END $$;";
        diesel::sql_query(drop_all_tables).execute(conn).await?;
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
        diesel::sql_query(drop_all_procedures).execute(conn).await?;
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
        diesel::sql_query(drop_all_functions).execute(conn).await?;
        info!("Database cleared.");
        Ok(())
    }

    pub async fn run_migrations(conn: Connection<'static>) -> Result<(), anyhow::Error> {
        info!("Running migrations ...");
        conn.run_pending_migrations(MIGRATIONS)
            .await
            .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::{Connection, ConnectionPool};
    use crate::db::{
        check_db_migration_consistency, check_db_migration_consistency_impl, reset_database,
        ConnectionPoolConfig, MIGRATIONS,
    };
    use diesel::migration::{Migration, MigrationSource};
    use diesel::pg::Pg;
    use diesel_migrations::MigrationHarness;
    use sui_pg_db::temp::TempDb;

    // Check that the migration records in the database created from the local schema
    // pass the consistency check.
    #[tokio::test]
    async fn db_migration_consistency_smoke_test() {
        let database = TempDb::new().unwrap();
        let pool = ConnectionPool::new(
            database.database().url().to_owned(),
            ConnectionPoolConfig {
                pool_size: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        reset_database(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();
        check_db_migration_consistency(&mut pool.get().await.unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn db_migration_consistency_non_prefix_test() {
        let database = TempDb::new().unwrap();
        let pool = ConnectionPool::new(
            database.database().url().to_owned(),
            ConnectionPoolConfig {
                pool_size: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        reset_database(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();
        let mut connection = pool.get().await.unwrap();

        let mut sync_connection_wrapper =
            diesel_async::async_connection_wrapper::AsyncConnectionWrapper::<Connection>::from(
                pool.dedicated_connection().await.unwrap(),
            );

        tokio::task::spawn_blocking(move || {
            sync_connection_wrapper
                .revert_migration(MIGRATIONS.migrations().unwrap().last().unwrap())
                .unwrap();
        })
        .await
        .unwrap();
        // Local migrations is one record more than the applied migrations.
        // This will fail the consistency check since it's not a prefix.
        assert!(check_db_migration_consistency(&mut connection)
            .await
            .is_err());

        pool.dedicated_connection()
            .await
            .unwrap()
            .run_pending_migrations(MIGRATIONS)
            .await
            .unwrap();
        // After running pending migrations they should be consistent.
        check_db_migration_consistency(&mut connection)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn db_migration_consistency_prefix_test() {
        let database = TempDb::new().unwrap();
        let pool = ConnectionPool::new(
            database.database().url().to_owned(),
            ConnectionPoolConfig {
                pool_size: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        reset_database(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();

        let migrations: Vec<Box<dyn Migration<Pg>>> = MIGRATIONS.migrations().unwrap();
        let mut local_migrations: Vec<_> = migrations.iter().map(|m| m.name().version()).collect();
        local_migrations.pop();
        // Local migrations is one record less than the applied migrations.
        // This should pass the consistency check since it's still a prefix.
        check_db_migration_consistency_impl(&mut pool.get().await.unwrap(), local_migrations)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn db_migration_consistency_subset_test() {
        let database = TempDb::new().unwrap();
        let pool = ConnectionPool::new(
            database.database().url().to_owned(),
            ConnectionPoolConfig {
                pool_size: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        reset_database(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();

        let migrations: Vec<Box<dyn Migration<Pg>>> = MIGRATIONS.migrations().unwrap();
        let mut local_migrations: Vec<_> = migrations.iter().map(|m| m.name().version()).collect();
        local_migrations.remove(2);

        // Local migrations are missing one record compared to the applied migrations, which should
        // still be okay.
        check_db_migration_consistency_impl(&mut pool.get().await.unwrap(), local_migrations)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn temp_db_smoketest() {
        use crate::database::Connection;
        use diesel_async::RunQueryDsl;
        use sui_pg_db::temp::TempDb;

        telemetry_subscribers::init_for_testing();

        let db = TempDb::new().unwrap();
        let url = db.database().url();
        println!("url: {}", url.as_str());
        let mut connection = Connection::dedicated(url).await.unwrap();

        // Run a simple query to verify the db can properly be queried
        let resp = diesel::sql_query("SELECT datname FROM pg_database")
            .execute(&mut connection)
            .await
            .unwrap();
        println!("resp: {:?}", resp);
    }
}
