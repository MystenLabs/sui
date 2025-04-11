// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Deref, DerefMut};
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use diesel::migration::{MigrationSource, MigrationVersion};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel_async::async_connection_wrapper::AsyncConnectionWrapper;
use diesel_async::{
    pooled_connection::{
        bb8::{Pool, PooledConnection},
        AsyncDieselConnectionManager,
    },
    AsyncConnection, AsyncPgConnection, RunQueryDsl,
};
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection as StoreConnection, PrunerWatermark, ReaderWatermark, Store,
    TransactionalStore,
};
use sui_sql_macro::sql;
use tracing::info;
use url::Url;

use crate::schema::watermarks;
use crate::FieldCount;

#[derive(clap::Args, Debug, Clone)]
pub struct DbArgs {
    /// Number of connections to keep in the pool.
    #[arg(long, default_value_t = Self::default().db_connection_pool_size)]
    pub db_connection_pool_size: u32,

    /// Time spent waiting for a connection from the pool to become available, in milliseconds.
    #[arg(long, default_value_t = Self::default().connection_timeout_ms)]
    pub connection_timeout_ms: u64,
}

/// Wrapper struct over a diesel async connection pool.
#[derive(Clone)]
pub struct Db {
    read_only: bool,
    pool: Pool<AsyncPgConnection>,
}

pub type ManagedConnection = AsyncPgConnection;
/// Type alias for a connection from the pool.
pub type Connection<'p> = PooledConnection<'p, ManagedConnection>;

/// Wrapper struct over the `Connection` type alias for dealing with the `Store` trait.
pub struct DbConnection<'a>(Connection<'a>);

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub struct StoredWatermark {
    pub pipeline: String,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
    pub reader_lo: i64,
    pub pruner_timestamp: NaiveDateTime,
    pub pruner_hi: i64,
}

impl DbArgs {
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_millis(self.connection_timeout_ms)
    }
}

impl Db {
    /// Construct a new DB connection pool talking to the database at `database_url` that supports
    /// write and reads. Instances of [Db] can be cloned to share access to the same pool.
    pub async fn for_write(database_url: Url, config: DbArgs) -> anyhow::Result<Self> {
        Ok(Self {
            read_only: false,
            pool: pool(database_url, config).await?,
        })
    }

    /// Construct a new DB connection pool talking to the database at `database_url` that defaults
    /// to read-only transactions. Instances of [Db] can be cloned to share access to the same
    /// pool.
    pub async fn for_read(database_url: Url, config: DbArgs) -> anyhow::Result<Self> {
        Ok(Self {
            read_only: true,
            pool: pool(database_url, config).await?,
        })
    }

    /// Retrieves a connection from the pool. Can fail with a timeout if a connection cannot be
    /// established before the [DbArgs::connection_timeout] has elapsed.
    pub async fn connection(&self) -> anyhow::Result<Connection<'_>> {
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

impl Default for DbArgs {
    fn default() -> Self {
        Self {
            db_connection_pool_size: 100,
            connection_timeout_ms: 60_000,
        }
    }
}

impl<'a> Deref for DbConnection<'a> {
    type Target = Connection<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DbConnection<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[async_trait]
impl StoreConnection for DbConnection<'_> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let watermark: Option<StoredWatermark> = watermarks::table
            .select(StoredWatermark::as_select())
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await
            .optional()
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive as u64,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as u64,
                tx_hi: watermark.tx_hi as u64,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let watermark: Option<StoredWatermark> = watermarks::table
            .select(StoredWatermark::as_select())
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await
            .optional()
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(ReaderWatermark {
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as u64,
                reader_lo: watermark.reader_lo as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        // Create a StoredWatermark directly from CommitterWatermark
        let stored_watermark = StoredWatermark {
            pipeline: pipeline.to_string(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive as i64,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as i64,
            tx_hi: watermark.tx_hi as i64,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive as i64,
            reader_lo: 0,
            pruner_timestamp: NaiveDateTime::UNIX_EPOCH,
            pruner_hi: 0,
        };

        use diesel::query_dsl::methods::FilterDsl;
        Ok(diesel::insert_into(watermarks::table)
            .values(&stored_watermark)
            // There is an existing entry, so only write the new `hi` values
            .on_conflict(watermarks::pipeline)
            .do_update()
            .set((
                watermarks::epoch_hi_inclusive.eq(stored_watermark.epoch_hi_inclusive),
                watermarks::checkpoint_hi_inclusive.eq(stored_watermark.checkpoint_hi_inclusive),
                watermarks::tx_hi.eq(stored_watermark.tx_hi),
                watermarks::timestamp_ms_hi_inclusive
                    .eq(stored_watermark.timestamp_ms_hi_inclusive),
            ))
            .filter(
                watermarks::checkpoint_hi_inclusive.lt(stored_watermark.checkpoint_hi_inclusive),
            )
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set((
                watermarks::reader_lo.eq(reader_lo as i64),
                watermarks::pruner_timestamp.eq(diesel::dsl::now),
            ))
            .filter(watermarks::pipeline.eq(pipeline))
            .filter(watermarks::reader_lo.lt(reader_lo as i64))
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        //     |---------- + delay ---------------------|
        //                             |--- wait_for ---|
        //     |-----------------------|----------------|
        //     ^                       ^
        //     pruner_timestamp        NOW()
        let wait_for = sql!(as BigInt,
            "CAST({BigInt} + 1000 * EXTRACT(EPOCH FROM pruner_timestamp - NOW()) AS BIGINT)",
            delay.as_millis() as i64,
        );

        let watermark: Option<(i64, i64, i64)> = watermarks::table
            .select((wait_for, watermarks::pruner_hi, watermarks::reader_lo))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await
            .optional()
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(PrunerWatermark {
                wait_for_ms: watermark.0 as u64,
                pruner_hi: watermark.1 as u64,
                reader_lo: watermark.2 as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set(watermarks::pruner_hi.eq(pruner_hi as i64))
            .filter(watermarks::pipeline.eq(pipeline))
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
    }
}

#[async_trait]
impl Store for Db {
    type Connection<'c> = DbConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        let conn = self.connection().await?;
        Ok(DbConnection(conn))
    }
}

#[async_trait]
impl TransactionalStore for Db {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = Store::connect(self).await?;
        AsyncConnection::transaction(&mut conn, |conn| f(conn)).await
    }
}

/// Drop all tables, and re-run migrations if supplied.
pub async fn reset_database<S: MigrationSource<Pg> + Send + Sync + 'static>(
    database_url: Url,
    db_config: DbArgs,
    migrations: Option<S>,
) -> anyhow::Result<()> {
    let db = Db::for_write(database_url, db_config).await?;
    db.clear_database().await?;
    if let Some(migrations) = migrations {
        db.run_migrations(migrations).await?;
    }

    Ok(())
}

async fn pool(database_url: Url, args: DbArgs) -> anyhow::Result<Pool<AsyncPgConnection>> {
    let manager = AsyncDieselConnectionManager::new(database_url.as_str());

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

    use crate::temp;

    #[tokio::test]
    async fn temp_db_smoketest() {
        telemetry_subscribers::init_for_testing();
        let db = temp::TempDb::new().unwrap();
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

    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        cnt: i64,
    }

    #[tokio::test]
    async fn test_reset_database_skip_migrations() {
        let temp_db = temp::TempDb::new().unwrap();
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

        reset_database::<EmbeddedMigrations>(url.clone(), DbArgs::default(), None)
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
}
