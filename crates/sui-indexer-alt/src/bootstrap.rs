// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::Indexer;
use anyhow::{Context, Result, bail};
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_framework::types::{
    full_checkpoint_content::CheckpointData,
    sui_system_state::{SuiSystemStateTrait, get_sui_system_state},
    transaction::{TransactionDataAPI, TransactionKind},
};
use sui_indexer_alt_schema::{
    checkpoints::StoredGenesis,
    epochs::StoredEpochStart,
    schema::{kv_epoch_starts, kv_genesis},
};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct BootstrapGenesis {
    pub stored_genesis: StoredGenesis,
    pub stored_epoch_start: StoredEpochStart,
}

/// Ensures the genesis table has been populated before the rest of the indexer is run, and returns
/// the information stored there. If the database has been bootstrapped before, this function will
/// simply read the previously bootstrapped information. Otherwise, it will wait until the first
/// checkpoint is available and extract the necessary information from there.
///
/// Can be cancelled via the `cancel` token, or through an interrupt signal (which will also cancel
/// the token).
pub async fn bootstrap(
    indexer: &Indexer<Db>,
    retry_interval: Duration,
    cancel: CancellationToken,
    bootstrap_genesis: Option<BootstrapGenesis>,
) -> Result<StoredGenesis> {
    info!("Bootstrapping indexer with genesis information");

    let Ok(mut conn) = indexer.store().connect().await else {
        bail!("Bootstrap failed to get connection for DB");
    };

    // 1. If the row has already been written, return it.
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

    let BootstrapGenesis {
        stored_genesis,
        stored_epoch_start,
    } = match bootstrap_genesis {
        // 2. If genesis is provided, use it to bootstrap.
        Some(bootstrap_genesis) => bootstrap_genesis,
        // 3. Otherwise, extract the necessary information from the genesis checkpoint:
        //
        // - Get the Genesis system transaction from the genesis checkpoint.
        // - Get the system state object that was written out by the system transaction.
        None => {
            let genesis_checkpoint = tokio::select! {
                cp = indexer.ingestion_client().wait_for(0, retry_interval) =>
                    cp.context("Failed to fetch genesis checkpoint")?,
                _ = cancel.cancelled() => {
                    bail!("Cancelled before genesis checkpoint was available");
                }
            };

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

            let sui_system_state =
                get_sui_system_state(&genesis_transaction.output_objects.as_slice())
                    .context("Failed to get Genesis SystemState")?;

            let stored_genesis = StoredGenesis {
                genesis_digest: checkpoint_summary.digest().inner().to_vec(),
                initial_protocol_version: sui_system_state.protocol_version() as i64,
            };
            let stored_epoch_start = StoredEpochStart {
                epoch: 0,
                protocol_version: sui_system_state.protocol_version() as i64,
                cp_lo: 0,
                start_timestamp_ms: sui_system_state.epoch_start_timestamp_ms() as i64,
                reference_gas_price: sui_system_state.reference_gas_price() as i64,
                system_state: bcs::to_bytes(&sui_system_state)
                    .context("Failed to serialize SystemState")?,
            };

            BootstrapGenesis {
                stored_genesis,
                stored_epoch_start,
            }
        }
    };

    info!(
        chain = stored_genesis.chain()?.as_str(),
        protocol = ?stored_genesis.initial_protocol_version(),
        "Bootstrapped indexer",
    );

    diesel::insert_into(kv_genesis::table)
        .values(&stored_genesis)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .context("Failed to write genesis record")?;

    diesel::insert_into(kv_epoch_starts::table)
        .values(&stored_epoch_start)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .context("Failed to write genesis epoch start record")?;

    Ok(stored_genesis)
}
