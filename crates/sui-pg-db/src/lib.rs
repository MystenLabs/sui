// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use diesel::ConnectionError;
use diesel::migration::Migration;
use diesel::migration::MigrationSource;
use diesel::migration::MigrationVersion;
use diesel::pg::Pg;
use diesel_async::RunQueryDsl;
use diesel_async::async_connection_wrapper::AsyncConnectionWrapper;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::ManagerConfig;
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::bb8::PooledConnection;
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::embed_migrations;
use futures::FutureExt;
use prometheus::Registry;
use tracing::info;
use url::Url;

use crate::tls::AsyncPgConnectionWithId;
use crate::tls::build_tls_config;
use crate::tls::establish_tls_connection;

mod metrics;
mod model;
pub mod query;
pub mod schema;
pub mod store;
pub mod temp;
mod tls;

use crate::metrics::PoolMetrics;
pub use sui_field_count::FieldCount;
pub use sui_sql_macro::sql;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[derive(clap::Args, Debug, Clone)]
pub struct DbArgs {
    /// Number of connections to keep in the pool.
    #[arg(long, default_value_t = Self::default().db_connection_pool_size)]
    pub db_connection_pool_size: u32,

    /// Time spent waiting for a connection from the pool to become available, in milliseconds.
    #[arg(long, default_value_t = Self::default().db_connection_timeout_ms)]
    pub db_connection_timeout_ms: u64,

    #[arg(long)]
    /// Time spent waiting for statements to complete, in milliseconds.
    pub db_statement_timeout_ms: Option<u64>,

    #[arg(long)]
    /// Enable server certificate verification. By default, this is set to false to match the
    /// default behavior of libpq.
    pub tls_verify_cert: bool,

    #[arg(long)]
    /// Path to a custom CA certificate to use for server certificate verification.
    pub tls_ca_cert_path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct Db {
    pool: Pool<AsyncPgConnectionWithId>,
    pool_metrics: Option<Arc<PoolMetrics>>,
}

// Ensures that unacquired_canceled is incremented if neither acquired nor unacquired_error are so
// that one of acquired, unacquired_error, unacquired_canceled is incremented for every requested
// connection. This is used to be able to calculate the number of pending connections:
// pending = requested - (acquired + unacquired_error + unacquired_canceled)
struct CancelGuard<'m>(Option<&'m PoolMetrics>);

/// Wrapper struct over the remote `PooledConnection` type for dealing with the `Store` trait.
pub struct Connection<'a>(PooledConnection<'a, AsyncPgConnectionWithId>);

impl DbArgs {
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_millis(self.db_connection_timeout_ms)
    }

    pub fn statement_timeout(&self) -> Option<Duration> {
        self.db_statement_timeout_ms.map(Duration::from_millis)
    }
}

impl Db {
    /// Construct a new DB connection pool talking to the database at `database_url` that supports
    /// write and reads. Instances of [Db] can be cloned to share access to the same pool.
    pub async fn for_write(database_url: Url, config: DbArgs) -> anyhow::Result<Self> {
        Self::new(database_url, config, false).await
    }

    /// Construct a new DB connection pool talking to the database at `database_url` that defaults
    /// to read-only transactions. Instances of [Db] can be cloned to share access to the same
    /// pool.
    pub async fn for_read(database_url: Url, config: DbArgs) -> anyhow::Result<Self> {
        Self::new(database_url, config, true).await
    }

    async fn new(database_url: Url, db_args: DbArgs, read_only: bool) -> anyhow::Result<Self> {
        Ok(Db {
            pool: pool(database_url, db_args, read_only).await?,
            pool_metrics: None,
        })
    }

    pub fn register_metrics(
        mut self,
        prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let pool_metrics = PoolMetrics::new(prefix, registry)?;
        self.pool_metrics = Some(pool_metrics);
        Ok(self)
    }

    /// Retrieves a connection from the pool. Can fail with a timeout if a connection cannot be
    /// established before the [DbArgs::connection_timeout] has elapsed.
    pub async fn connect(&self) -> anyhow::Result<Connection<'_>> {
        if let Some(pool_metrics) = &self.pool_metrics {
            let guard = CancelGuard::request(pool_metrics);
            match self.pool.get().await {
                Ok(c) => {
                    guard.acquired();
                    Ok(Connection(c))
                }
                Err(e) => {
                    guard.unacquired_error();
                    Err(e.into())
                }
            }
        } else {
            Ok(Connection(self.pool.get().await?))
        }
    }

    /// Statistics about the connection pool
    pub fn state(&self) -> bb8::State {
        self.pool.state()
    }

    pub fn pool_metrics(&self) -> Option<&PoolMetrics> {
        self.pool_metrics.as_ref().map(|p| p.as_ref())
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
    pub async fn run_migrations(
        &self,
        migrations: Option<&'static EmbeddedMigrations>,
    ) -> anyhow::Result<Vec<MigrationVersion<'static>>> {
        use diesel_migrations::MigrationHarness;

        let merged_migrations = merge_migrations(migrations);

        info!("Running migrations ...");
        let conn = self.pool.dedicated_connection().await?;
        let mut wrapper: AsyncConnectionWrapper<AsyncPgConnectionWithId> =
            AsyncConnectionWrapper::from(conn);

        let finished_migrations = tokio::task::spawn_blocking(move || {
            wrapper
                .run_pending_migrations(merged_migrations)
                .map(|versions| versions.iter().map(MigrationVersion::as_owned).collect())
        })
        .await?
        .map_err(|e| anyhow!("Failed to run migrations: {:?}", e))?;

        info!("Migrations complete.");
        Ok(finished_migrations)
    }
}

impl<'m> CancelGuard<'m> {
    fn request(stats: &'m PoolMetrics) -> Self {
        stats.requested.inc();
        Self(Some(stats))
    }

    fn acquired(mut self) {
        if let Some(m) = self.0.take() {
            m.acquired.inc()
        }
    }

    fn unacquired_error(mut self) {
        if let Some(m) = self.0.take() {
            m.unacquired_error.inc()
        }
    }
}

impl Default for DbArgs {
    fn default() -> Self {
        Self {
            db_connection_pool_size: 100,
            db_connection_timeout_ms: 60_000,
            db_statement_timeout_ms: None,
            tls_verify_cert: false,
            tls_ca_cert_path: None,
        }
    }
}

impl<'m> Drop for CancelGuard<'m> {
    fn drop(&mut self) {
        if let Some(m) = self.0.take() {
            m.unacquired_canceled.inc()
        }
    }
}

/// Drop all tables, and re-run migrations if supplied.
pub async fn reset_database(
    database_url: Url,
    db_args: DbArgs,
    migrations: Option<&'static EmbeddedMigrations>,
) -> anyhow::Result<()> {
    let db = Db::for_write(database_url, db_args).await?;
    db.clear_database().await?;
    if let Some(migrations) = migrations {
        db.run_migrations(Some(migrations)).await?;
    }

    Ok(())
}

impl<'a> Deref for Connection<'a> {
    type Target = PooledConnection<'a, AsyncPgConnectionWithId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Connection<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

async fn pool(
    database_url: Url,
    args: DbArgs,
    read_only: bool,
) -> anyhow::Result<Pool<AsyncPgConnectionWithId>> {
    let statement_timeout = args.statement_timeout();

    // Build TLS configuration once
    let tls_config = build_tls_config(args.tls_verify_cert, args.tls_ca_cert_path.clone())?;

    let mut config = ManagerConfig::default();

    config.custom_setup = Box::new(move |url| {
        let tls_config = tls_config.clone();

        async move {
            let mut conn = establish_tls_connection(url, tls_config).await?;

            if let Some(timeout) = statement_timeout {
                diesel::sql_query(format!("SET statement_timeout = {}", timeout.as_millis()))
                    .execute(&mut conn)
                    .await
                    .map_err(ConnectionError::CouldntSetupConfiguration)?;
            }

            if read_only {
                diesel::sql_query("SET default_transaction_read_only = 'on'")
                    .execute(&mut conn)
                    .await
                    .map_err(ConnectionError::CouldntSetupConfiguration)?;
            }

            Ok(conn)
        }
        .boxed()
    });

    let manager = AsyncDieselConnectionManager::new_with_config(database_url.as_str(), config);

    Ok(Pool::builder()
        .max_size(args.db_connection_pool_size)
        .connection_timeout(args.connection_timeout())
        .build(manager)
        .await?)
}

/// Returns new migrations derived from the combination of provided migrations and migrations
/// defined in this crate.
pub fn merge_migrations(
    migrations: Option<&'static EmbeddedMigrations>,
) -> impl MigrationSource<Pg> + Send + Sync + 'static {
    struct Migrations(Option<&'static EmbeddedMigrations>);
    impl MigrationSource<Pg> for Migrations {
        fn migrations(&self) -> diesel::migration::Result<Vec<Box<dyn Migration<Pg>>>> {
            let mut migrations = MIGRATIONS.migrations()?;
            if let Some(more_migrations) = self.0 {
                migrations.extend(more_migrations.migrations()?);
            }
            Ok(migrations)
        }
    }

    Migrations(migrations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temp::TempDb;
    use anyhow::Error;
    use diesel::prelude::QueryableByName;
    use tokio::spawn;
    use tokio::time::timeout;

    struct MetricTest {
        db: Db,
        _temp_db: TempDb,
        _registry: Registry,
    }

    impl MetricTest {
        async fn new(db_connection_timeout: Duration) -> Arc<Self> {
            let temp_db = TempDb::new().unwrap();
            let url = temp_db.database().url();
            let db_args = DbArgs {
                db_connection_pool_size: 1,
                db_connection_timeout_ms: db_connection_timeout.as_millis() as u64,
                ..Default::default()
            };
            let registry = Registry::new();
            let db = Db::for_read(url.clone(), db_args)
                .await
                .unwrap()
                .register_metrics(None, &registry)
                .unwrap();
            Arc::new(Self {
                db,
                _temp_db: temp_db,
                _registry: registry,
            })
        }

        fn pool_metrics(&self) -> &PoolMetrics {
            self.db.pool_metrics().unwrap()
        }

        async fn select_sleep(&self, duration: Duration) -> Result<(), Error> {
            let mut conn = self.db.connect().await?;
            let duration_s = duration.as_secs_f64();
            diesel::sql_query(format!("SELECT pg_sleep({duration_s});"))
                .execute(&mut conn)
                .await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn temp_db_smoketest() {
        telemetry_subscribers::init_for_testing();
        let db = TempDb::new().unwrap();
        let url = db.database().url();

        info!(%url);
        let db = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();
        let mut conn = db.connect().await.unwrap();

        // Run a simple query to verify the db can properly be queried
        let resp = diesel::sql_query("SELECT datname FROM pg_database")
            .execute(&mut conn)
            .await
            .unwrap();

        info!(?resp);
    }

    #[derive(Debug, QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        cnt: i64,
    }

    #[tokio::test]
    async fn test_reset_database_skip_migrations() {
        let temp_db = TempDb::new().unwrap();
        let url = temp_db.database().url();

        let db = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();
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

        reset_database(url.clone(), DbArgs::default(), None)
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
        let temp_db = TempDb::new().unwrap();
        let url = temp_db.database().url();

        let writer = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();
        let reader = Db::for_read(url.clone(), DbArgs::default()).await.unwrap();

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

    #[tokio::test]
    async fn test_statement_timeout() {
        let temp_db = TempDb::new().unwrap();
        let url = temp_db.database().url();

        let reader = Db::for_read(
            url.clone(),
            DbArgs {
                db_statement_timeout_ms: Some(200),
                ..DbArgs::default()
            },
        )
        .await
        .unwrap();

        {
            // A simple query should not timeout
            let mut conn = reader.connect().await.unwrap();
            let cnt: CountResult = diesel::sql_query("SELECT 1::BIGINT AS cnt")
                .get_result(&mut conn)
                .await
                .unwrap();

            assert_eq!(cnt.cnt, 1);
        }

        {
            // A query that waits a bit, which should cause a timeout
            let mut conn = reader.connect().await.unwrap();
            diesel::sql_query("SELECT PG_SLEEP(2), 1::BIGINT AS cnt")
                .get_result::<CountResult>(&mut conn)
                .await
                .expect_err("This request should fail because of a timeout");
        }
    }

    #[tokio::test]
    async fn test_unacquired_error() {
        let db_connection_timeout = Duration::from_millis(500);
        let metric_test = MetricTest::new(db_connection_timeout).await;

        let metric_test_clone = metric_test.clone();
        let task1 = spawn(async move {
            metric_test_clone
                .select_sleep(db_connection_timeout + Duration::from_millis(500))
                .await
        });
        // 1st task takes longer than db_connection_timeout so 2nd task times out
        let metric_test_clone = metric_test.clone();
        let task2 = spawn(async move {
            // sleep duration does not matter because it will never execute
            metric_test_clone.select_sleep(Duration::ZERO).await
        });
        assert!(task1.await.unwrap().is_ok());
        assert!(task2.await.unwrap().is_err());

        let PoolMetrics {
            requested,
            acquired,
            unacquired_error,
            unacquired_canceled,
        } = metric_test.pool_metrics();
        assert_eq!(requested.get(), 2);
        assert_eq!(acquired.get(), 1);
        assert_eq!(unacquired_error.get(), 1);
        assert_eq!(unacquired_canceled.get(), 0);
    }

    #[tokio::test]
    async fn test_unacquired_canceled() {
        let task_timeout = Duration::from_millis(500);
        let sleep_timeout = task_timeout + Duration::from_millis(500);
        let db_connection_timeout = sleep_timeout + Duration::from_millis(500);
        let metric_test = MetricTest::new(db_connection_timeout).await;

        let metric_test_clone = metric_test.clone();
        let task1 = spawn(async move {
            metric_test_clone
                .select_sleep(db_connection_timeout + Duration::from_millis(500))
                .await
        });
        let metric_test_clone = metric_test.clone();
        // 1st task takes longer than task_timeout so 2nd task times out
        let task2 = spawn(async move {
            // sleep duration does not matter because it will never execute
            timeout(task_timeout, metric_test_clone.select_sleep(Duration::ZERO)).await
        });
        assert!(task1.await.unwrap().is_ok());
        assert!(task2.await.unwrap().is_err());

        let PoolMetrics {
            requested,
            acquired,
            unacquired_error,
            unacquired_canceled,
        } = metric_test.pool_metrics();
        assert_eq!(requested.get(), 2);
        assert_eq!(acquired.get(), 1);
        assert_eq!(unacquired_error.get(), 0);
        assert_eq!(unacquired_canceled.get(), 1);
    }
}
