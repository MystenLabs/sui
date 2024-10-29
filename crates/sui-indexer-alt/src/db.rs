// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use diesel::migration::MigrationVersion;
use diesel_async::async_connection_wrapper::AsyncConnectionWrapper;
use diesel_async::{
    pooled_connection::{
        bb8::{Pool, PooledConnection, RunError},
        AsyncDieselConnectionManager, PoolError,
    },
    AsyncPgConnection, RunQueryDsl,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use std::time::Duration;
use tracing::info;
use url::Url;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

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

    async fn clear_database(&self) -> Result<(), anyhow::Error> {
        info!("Clearing the database...");
        let mut conn = self.connect().await?;
        let drop_all_tables = "
        DO $$ DECLARE
            r RECORD;
        BEGIN
        FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public')
            LOOP
                EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
            END LOOP;
        END $$;";
        diesel::sql_query(drop_all_tables)
            .execute(&mut conn)
            .await?;
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
        diesel::sql_query(drop_all_procedures)
            .execute(&mut conn)
            .await?;
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
        diesel::sql_query(drop_all_functions)
            .execute(&mut conn)
            .await?;
        info!("Database cleared.");
        Ok(())
    }

    pub(crate) async fn run_migrations(
        &self,
    ) -> Result<Vec<MigrationVersion<'static>>, anyhow::Error> {
        use diesel_migrations::MigrationHarness;

        info!("Running migrations ...");
        let conn = self.pool.dedicated_connection().await?;
        let mut wrapper: AsyncConnectionWrapper<AsyncPgConnection> =
            diesel_async::async_connection_wrapper::AsyncConnectionWrapper::from(conn);

        let finished_migrations = tokio::task::spawn_blocking(move || {
            wrapper
                .run_pending_migrations(MIGRATIONS)
                .map(|versions| versions.iter().map(MigrationVersion::as_owned).collect())
        })
        .await?
        .map_err(|e| anyhow!("Failed to run migrations: {:?}", e))?;
        info!("Migrations complete.");
        Ok(finished_migrations)
    }
}

/// Drop all tables and rerunning migrations.
pub async fn reset_database(
    db_config: DbConfig,
    skip_migrations: bool,
) -> Result<(), anyhow::Error> {
    let db = Db::new(db_config).await?;
    db.clear_database().await?;
    if !skip_migrations {
        db.run_migrations().await?;
    }
    Ok(())
}
