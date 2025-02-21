// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use diesel::migration::{MigrationSource, MigrationVersion};
use diesel::pg::Pg;
use diesel_async::async_connection_wrapper::AsyncConnectionWrapper;
use diesel_async::{
    pooled_connection::{
        bb8::{Pool, PooledConnection},
        AsyncDieselConnectionManager,
    },
    AsyncPgConnection, RunQueryDsl,
};
use std::time::Duration;
use tracing::info;
use url::Url;

pub mod temp;

#[derive(clap::Args, Debug, Clone)]
pub struct DbArgs {
    /// The URL of the database to connect to.
    #[arg(long, default_value_t = Self::default().database_url)]
    pub database_url: Url,

    /// Number of connections to keep in the pool.
    #[arg(long, default_value_t = Self::default().db_connection_pool_size)]
    pub db_connection_pool_size: u32,

    /// Time spent waiting for a connection from the pool to become available, in milliseconds.
    #[arg(long, default_value_t = Self::default().connection_timeout_ms)]
    pub connection_timeout_ms: u64,
}

#[derive(Clone)]
pub struct Db {
    read_only: bool,
    pool: Pool<AsyncPgConnection>,
}

pub type ManagedConnection = AsyncPgConnection;
pub type Connection<'p> = PooledConnection<'p, ManagedConnection>;

impl DbArgs {
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_millis(self.connection_timeout_ms)
    }
}

impl Db {
    /// Construct a new DB connection pool that supports write and reads. Instances of [Db] can be
    /// cloned to share access to the same pool.
    pub async fn for_write(config: DbArgs) -> anyhow::Result<Self> {
        Ok(Self {
            read_only: false,
            pool: pool(config).await?,
        })
    }

    /// Construct a new DB connection pool that defaults to read-only transactions. Instances of
    /// [Db] can be cloned to share access to the same pool.
    pub async fn for_read(config: DbArgs) -> anyhow::Result<Self> {
        Ok(Self {
            read_only: true,
            pool: pool(config).await?,
        })
    }

    /// Retrieves a connection from the pool. Can fail with a timeout if a connection cannot be
    /// established before the [DbArgs::connection_timeout] has elapsed.
    pub async fn connect(&self) -> anyhow::Result<Connection<'_>> {
        let mut conn = self.pool.get().await?;
        if self.read_only {
            diesel::sql_query("SET default_transaction_read_only = 'on'")
                .execute(&mut conn)
                .await?;
        }

        Ok(conn)
    }

    /// Statistics about the connection pool
    pub fn state(&self) -> bb8::State {
        self.pool.state()
    }

    async fn clear_database(&self) -> anyhow::Result<()> {
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

    /// Run migrations on the database. Use Diesel's `embed_migrations!` macro to generate the
    /// `migrations` parameter for your indexer.
    pub async fn run_migrations<S: MigrationSource<Pg> + Send + Sync + 'static>(
        &self,
        migrations: S,
    ) -> anyhow::Result<Vec<MigrationVersion<'static>>> {
        use diesel_migrations::MigrationHarness;

        info!("Running migrations ...");
        let conn = self.pool.dedicated_connection().await?;
        let mut wrapper: AsyncConnectionWrapper<AsyncPgConnection> =
            diesel_async::async_connection_wrapper::AsyncConnectionWrapper::from(conn);

        let finished_migrations = tokio::task::spawn_blocking(move || {
            wrapper
                .run_pending_migrations(migrations)
                .map(|versions| versions.iter().map(MigrationVersion::as_owned).collect())
        })
        .await?
        .map_err(|e| anyhow!("Failed to run migrations: {:?}", e))?;

        info!("Migrations complete.");
        Ok(finished_migrations)
    }
}

impl DbArgs {
    pub fn new_for_testing(database_url: Url) -> Self {
        Self {
            database_url,
            ..Default::default()
        }
    }
}

impl Default for DbArgs {
    fn default() -> Self {
        Self {
            database_url: Url::parse(
                "postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt",
            )
            .unwrap(),
            db_connection_pool_size: 100,
            connection_timeout_ms: 60_000,
        }
    }
}

/// Drop all tables, and re-run migrations if supplied.
pub async fn reset_database<S: MigrationSource<Pg> + Send + Sync + 'static>(
    db_config: DbArgs,
    migrations: Option<S>,
) -> anyhow::Result<()> {
    let db = Db::for_write(db_config).await?;
    db.clear_database().await?;
    if let Some(migrations) = migrations {
        db.run_migrations(migrations).await?;
    }

    Ok(())
}

async fn pool(args: DbArgs) -> anyhow::Result<Pool<AsyncPgConnection>> {
    let manager = AsyncDieselConnectionManager::new(args.database_url.as_str());

    Ok(Pool::builder()
        .max_size(args.db_connection_pool_size)
        .connection_timeout(args.connection_timeout())
        .build(manager)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::prelude::QueryableByName;
    use diesel_async::RunQueryDsl;
    use diesel_migrations::EmbeddedMigrations;

    #[tokio::test]
    async fn temp_db_smoketest() {
        telemetry_subscribers::init_for_testing();
        let db = temp::TempDb::new().unwrap();
        let url = db.database().url();

        info!(%url);
        let db_args = DbArgs {
            database_url: url.clone(),
            ..Default::default()
        };

        let db = Db::for_write(db_args).await.unwrap();
        let mut conn = db.connect().await.unwrap();

        // Run a simple query to verify the db can properly be queried
        let resp = diesel::sql_query("SELECT datname FROM pg_database")
            .execute(&mut conn)
            .await
            .unwrap();

        info!(?resp);
    }

    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        cnt: i64,
    }

    #[tokio::test]
    async fn test_reset_database_skip_migrations() {
        let temp_db = temp::TempDb::new().unwrap();
        let url = temp_db.database().url();

        let db_args = DbArgs {
            database_url: url.clone(),
            ..Default::default()
        };

        let db = Db::for_write(db_args.clone()).await.unwrap();
        let mut conn = db.connect().await.unwrap();
        diesel::sql_query("CREATE TABLE test_table (id INTEGER PRIMARY KEY)")
            .execute(&mut conn)
            .await
            .unwrap();
        let cnt = diesel::sql_query(
            "SELECT COUNT(*) as cnt FROM information_schema.tables WHERE table_name = 'test_table'",
        )
        .get_result::<CountResult>(&mut conn)
        .await
        .unwrap();
        assert_eq!(cnt.cnt, 1);

        reset_database::<EmbeddedMigrations>(db_args, None)
            .await
            .unwrap();

        let mut conn = db.connect().await.unwrap();
        let cnt: CountResult = diesel::sql_query(
            "SELECT COUNT(*) as cnt FROM information_schema.tables WHERE table_name = 'test_table'",
        )
        .get_result(&mut conn)
        .await
        .unwrap();
        assert_eq!(cnt.cnt, 0);
    }

    #[tokio::test]
    async fn test_read_only() {
        let temp_db = temp::TempDb::new().unwrap();
        let url = temp_db.database().url();

        let db_args = DbArgs {
            database_url: url.clone(),
            ..Default::default()
        };

        let writer = Db::for_write(db_args.clone()).await.unwrap();
        let reader = Db::for_read(db_args).await.unwrap();

        {
            // Create a table
            let mut conn = writer.connect().await.unwrap();
            diesel::sql_query("CREATE TABLE test_table (id INTEGER PRIMARY KEY)")
                .execute(&mut conn)
                .await
                .unwrap();
        }

        {
            // Try an insert into it using the read-only connection, which should fail
            let mut conn = reader.connect().await.unwrap();
            let result = diesel::sql_query("INSERT INTO test_table (id) VALUES (1)")
                .execute(&mut conn)
                .await;
            assert!(result.is_err());
        }

        {
            // Try and select from it using the read-only connection, which should succeed, but
            // return no results.
            let mut conn = reader.connect().await.unwrap();
            let cnt: CountResult = diesel::sql_query("SELECT COUNT(*) as cnt FROM test_table")
                .get_result(&mut conn)
                .await
                .unwrap();
            assert_eq!(cnt.cnt, 0);
        }

        {
            // Then try to write to it using the write connection, which should succeed
            let mut conn = writer.connect().await.unwrap();
            diesel::sql_query("INSERT INTO test_table (id) VALUES (1)")
                .execute(&mut conn)
                .await
                .unwrap();
        }

        {
            // Finally, try to read from it using the read-only connection, which should now return
            // results.
            let mut conn = reader.connect().await.unwrap();
            let cnt: CountResult = diesel::sql_query("SELECT COUNT(*) as cnt FROM test_table")
                .get_result(&mut conn)
                .await
                .unwrap();
            assert_eq!(cnt.cnt, 1);
        }
    }
}
