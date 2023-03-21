// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::store::PgIndexerStore;
use crate::{new_pg_connection_pool, Indexer, IndexerConfig, PgPoolConnection};
use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{PgConnection, RunQueryDsl};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use prometheus::Registry;
use tokio::task::JoinHandle;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Resets the database by reverting all migrations and reapplying them.
///
/// If `drop_all` is set to `true`, the function will drop all tables in the database before
/// resetting the migrations. This option is destructive and will result in the loss of all
/// data in the tables. Use with caution, especially in production environments.
pub fn reset_database(conn: &mut PgPoolConnection, drop_all: bool) -> Result<(), anyhow::Error> {
    if drop_all {
        drop_all_tables(conn)
            .map_err(|e| anyhow!("Encountering error when dropping all tables {e}"))?;
    } else {
        conn.revert_all_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("Error reverting all migrations {e}"))?;
    }

    conn.run_migrations(&MIGRATIONS.migrations().unwrap())
        .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
    Ok(())
}

pub fn drop_all_tables(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
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
    Ok(())
}

/// Spawns an indexer thread with provided Postgres DB url
pub async fn start_test_indexer(
    config: IndexerConfig,
    reset: bool,
) -> Result<(PgIndexerStore, JoinHandle<Result<(), IndexerError>>), anyhow::Error> {
    let pg_connection_pool = new_pg_connection_pool(&config.base_connection_url())
        .await
        .map_err(|e| anyhow!("unable to connect to Postgres, is it running? {e}"))?;
    if reset {
        reset_database(
            &mut pg_connection_pool
                .get()
                .map_err(|e| anyhow!("Fail to get pg_connection_pool {e}"))?,
            true,
        )?;
    }
    let store = PgIndexerStore::new(pg_connection_pool);

    let registry = Registry::default();
    let store_clone = store.clone();
    let handle = tokio::spawn(async move { Indexer::start(&config, &registry, store_clone).await });
    Ok((store, handle))
}
