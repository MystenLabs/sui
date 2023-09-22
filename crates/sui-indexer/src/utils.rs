// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::PgPoolConnection;
use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{PgConnection, RunQueryDsl};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
const MIGRATIONS_V2: EmbeddedMigrations = embed_migrations!("migrations_v2");

/// Resets the database by reverting all migrations and reapplying them.
///
/// If `drop_all` is set to `true`, the function will drop all tables in the database before
/// resetting the migrations. This option is destructive and will result in the loss of all
/// data in the tables. Use with caution, especially in production environments.
pub fn reset_database(
    conn: &mut PgPoolConnection,
    drop_all: bool,
    use_v2: bool,
) -> Result<(), anyhow::Error> {
    info!("Resetting database ...");
    let migration = if use_v2 { MIGRATIONS_V2 } else { MIGRATIONS };
    if drop_all {
        drop_all_tables(conn)
            .map_err(|e| anyhow!("Encountering error when dropping all tables {e}"))?;
    } else {
        conn.revert_all_migrations(migration)
            .map_err(|e| anyhow!("Error reverting all migrations {e}"))?;
    }
    let migration = if use_v2 { MIGRATIONS_V2 } else { MIGRATIONS };
    conn.run_migrations(&migration.migrations().unwrap())
        .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
    info!("Reset database complete.");
    Ok(())
}

pub fn drop_all_tables(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
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
