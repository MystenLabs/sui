// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Neg;

use anyhow::anyhow;
use diesel::migration::MigrationSource;
use diesel::{PgConnection, RunQueryDsl};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use jsonrpsee::http_client::HttpClient;
use sui_types::digests::ObjectDigest;
use tracing::info;

use sui_json_rpc::api::ReadApiClient;
use sui_json_rpc::{get_balance_changes, ObjectProvider};
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_json_rpc_types::{
    BalanceChange, SuiExecutionStatus, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_json_rpc_types::{ObjectChange, OwnedObjectRef, SuiObjectRef};
use sui_types::base_types::TransactionDigest;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::storage::{DeleteKind, WriteKind};

use crate::errors::IndexerError;
use crate::types::CheckpointTransactionBlockResponse;
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
    http_client: HttpClient,
    digests: Vec<TransactionDigest>,
) -> Result<Vec<CheckpointTransactionBlockResponse>, IndexerError> {
    let sui_transactions = http_client
        .multi_get_transaction_blocks(
            digests.clone(),
            // MUSTFIX(gegaowp): avoid double fetching both input and raw_input
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_input()
                    .with_effects()
                    .with_events()
                    .with_raw_input(),
            ),
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed to get transactions {:?} with error: {:?}",
                digests.clone(),
                e
            ))
        })?;
    let sui_full_transactions: Vec<CheckpointTransactionBlockResponse> = sui_transactions
        .into_iter()
        .map(CheckpointTransactionBlockResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            IndexerError::UnexpectedFullnodeResponseError(format!(
                "Unexpected None value in SuiTransactionBlockFullResponse with error {:?}",
                e
            ))
        })?;
    Ok(sui_full_transactions)
}

pub async fn get_balance_changes_from_effect<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    effects: &SuiTransactionBlockEffects,
) -> Result<Vec<BalanceChange>, E> {
    let gas_owner = effects.gas_object().owner;
    // Only charge gas when tx fails, skip all object parsing
    let gas_cost_summary: GasCostSummary = effects.gas_cost_summary().clone();
    if effects.status() != &SuiExecutionStatus::Success {
        return Ok(vec![BalanceChange {
            owner: gas_owner,
            coin_type: GAS::type_tag(),
            amount: gas_cost_summary.net_gas_usage().neg() as i128,
        }]);
    }

    let all_mutated: Vec<(ObjectID, SequenceNumber, Option<ObjectDigest>)> = effects
        .all_changed_objects()
        .into_iter()
        .map(|(owner_obj_ref, _)| {
            (
                owner_obj_ref.reference.object_id,
                owner_obj_ref.reference.version,
                Some(owner_obj_ref.reference.digest),
            )
        })
        .collect();
    // TODO: thread through input object digests here instead of passing None
    let modified_at_versions: Vec<(ObjectID, SequenceNumber, Option<ObjectDigest>)> = effects
        .modified_at_versions()
        .into_iter()
        .map(|(id, version)| (id, version, None))
        .collect();
    get_balance_changes(object_provider, &modified_at_versions, &all_mutated).await
}

pub async fn get_object_changes<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    sender: SuiAddress,
    modified_at_versions: &[(ObjectID, SequenceNumber)],
    all_changed_objects: Vec<(&OwnedObjectRef, WriteKind)>,
    all_deleted: Vec<(&SuiObjectRef, DeleteKind)>,
) -> Result<Vec<ObjectChange>, E> {
    let all_changed: Vec<(ObjectRef, Owner, WriteKind)> = all_changed_objects
        .into_iter()
        .map(|(obj_owner_ref, write_kind)| {
            (
                (
                    obj_owner_ref.reference.object_id,
                    obj_owner_ref.reference.version,
                    obj_owner_ref.reference.digest,
                ),
                obj_owner_ref.owner,
                write_kind,
            )
        })
        .collect();
    let all_changed_objects = all_changed
        .iter()
        .map(|(obj_ref, owner, write_kind)| (obj_ref, owner, *write_kind))
        .collect();

    let all_deleted: Vec<(ObjectRef, DeleteKind)> = all_deleted
        .into_iter()
        .map(|(obj_ref, delete_kind)| {
            (
                (obj_ref.object_id, obj_ref.version, obj_ref.digest),
                delete_kind,
            )
        })
        .collect();
    let all_deleted_objects = all_deleted
        .iter()
        .map(|(obj_ref, delete_kind)| (obj_ref, *delete_kind))
        .collect();

    sui_json_rpc::get_object_changes(
        object_provider,
        sender,
        modified_at_versions,
        all_changed_objects,
        all_deleted_objects,
    )
    .await
}
