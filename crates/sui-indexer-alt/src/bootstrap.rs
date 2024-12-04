// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{bail, Context, Result};
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::task::graceful_shutdown;
use sui_indexer_alt_schema::{
    checkpoints::StoredGenesis,
    epochs::StoredEpochStart,
    schema::{kv_epoch_starts, kv_genesis},
};
use sui_types::{
    full_checkpoint_content::CheckpointData,
    sui_system_state::{get_sui_system_state, SuiSystemStateTrait},
    transaction::{TransactionDataAPI, TransactionKind},
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::Indexer;

/// Ensures the genesis table has been populated before the rest of the indexer is run, and returns
/// the information stored there. If the database has been bootstrapped before, this function will
/// simply read the previously bootstrapped information. Otherwise, it will wait until the first
/// checkpoint is available and extract the necessary information from there.
///
/// Can be cancelled via the `cancel` token, or through an interrupt signal (which will also cancel
/// the token).
pub async fn bootstrap(
    indexer: &Indexer,
    retry_interval: Duration,
    cancel: CancellationToken,
) -> Result<StoredGenesis> {
    let Ok(mut conn) = indexer.db().connect().await else {
        bail!("Bootstrap failed to get connection for DB");
    };

    // If the row has already been written, return it.
    if let Some(genesis) = kv_genesis::table
        .select(StoredGenesis::as_select())
        .first(&mut conn)
        .await
        .optional()?
    {
        info!(
            chain = genesis.chain()?.as_str(),
            protocol = ?genesis.initial_protocol_version(),
            "Indexer already bootstrapped",
        );

        return Ok(genesis);
    }

    // Otherwise, extract the necessary information from the genesis checkpoint:
    //
    // - Get the Genesis system transaction from the genesis checkpoint.
    // - Get the system state object that was written out by the system transaction.
    let ingestion_client = indexer.ingestion_client().clone();
    let wait_cancel = cancel.clone();
    let genesis = tokio::spawn(async move {
        ingestion_client
            .wait_for(0, retry_interval, &wait_cancel)
            .await
    });

    let Some(genesis_checkpoint) = graceful_shutdown(vec![genesis], cancel).await.pop() else {
        bail!("Bootstrap cancelled");
    };

    let genesis_checkpoint = genesis_checkpoint.context("Failed to fetch genesis checkpoint")?;

    let CheckpointData {
        checkpoint_summary,
        transactions,
        ..
    } = genesis_checkpoint.as_ref();

    let Some(genesis_transaction) = transactions.iter().find(|tx| {
        matches!(
            tx.transaction.intent_message().value.kind(),
            TransactionKind::Genesis(_)
        )
    }) else {
        bail!("Could not find Genesis transaction");
    };

    let system_state = get_sui_system_state(&genesis_transaction.output_objects.as_slice())
        .context("Failed to get Genesis SystemState")?;

    let genesis = StoredGenesis {
        genesis_digest: checkpoint_summary.digest().inner().to_vec(),
        initial_protocol_version: system_state.protocol_version() as i64,
    };

    let epoch_start = StoredEpochStart {
        epoch: 0,
        protocol_version: system_state.protocol_version() as i64,
        cp_lo: 0,
        start_timestamp_ms: system_state.epoch_start_timestamp_ms() as i64,
        reference_gas_price: system_state.reference_gas_price() as i64,
        system_state: bcs::to_bytes(&system_state).context("Failed to serialize SystemState")?,
    };

    info!(
        chain = genesis.chain()?.as_str(),
        protocol = ?genesis.initial_protocol_version(),
        "Bootstrapped indexer",
    );

    diesel::insert_into(kv_genesis::table)
        .values(&genesis)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .context("Failed to write genesis record")?;

    diesel::insert_into(kv_epoch_starts::table)
        .values(&epoch_start)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .context("Failed to write genesis epoch start record")?;

    Ok(genesis)
}
