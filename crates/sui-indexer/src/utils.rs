// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{PgConnection, RunQueryDsl};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;

use sui_json_rpc_types::SuiTransactionResponseOptions;
use sui_sdk::apis::ReadApi as SuiReadApi;
use sui_types::base_types::TransactionDigest;

use crate::errors::IndexerError;
use crate::types::SuiTransactionFullResponse;
use crate::PgPoolConnection;

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

pub async fn multi_get_full_transactions(
    read_api: &SuiReadApi,
    digests: Vec<TransactionDigest>,
) -> Result<Vec<SuiTransactionFullResponse>, IndexerError> {
    let sui_transactions = read_api
        .multi_get_transactions_with_options(
            digests.clone(),
            // MUSTFIX(gegaowp): avoid double fetching both input and raw_input
            SuiTransactionResponseOptions::new()
                .with_input()
                .with_effects()
                .with_events()
                .with_raw_input(),
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed to get transactions {:?} with error: {:?}",
                digests.clone(),
                e
            ))
        })?;
    let sui_full_transactions: Vec<SuiTransactionFullResponse> = sui_transactions
        .into_iter()
        .map(SuiTransactionFullResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Unexpected None value in SuiTransactionFullResponse of digests {:?} with error {:?}",
                digests, e
            ))
        })?;
    Ok(sui_full_transactions)
}
