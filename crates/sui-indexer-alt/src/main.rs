// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::db::reset_database;
use sui_indexer_alt::{
    args::Args,
    handlers::{
        ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
        kv_objects::KvObjects, kv_transactions::KvTransactions, sum_obj_types::SumObjTypes,
        tx_affected_objects::TxAffectedObjects, tx_balance_changes::TxBalanceChanges,
    },
    Indexer,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let cancel = CancellationToken::new();

    match args.command {
        Command::Indexer(indexer_config) => {
            let mut indexer = Indexer::new(args.db_config, indexer_config, cancel.clone()).await?;

            indexer.concurrent_pipeline::<EvEmitMod>().await?;
            indexer.concurrent_pipeline::<EvStructInst>().await?;
            indexer.concurrent_pipeline::<KvCheckpoints>().await?;
            indexer.concurrent_pipeline::<KvObjects>().await?;
            indexer.concurrent_pipeline::<KvTransactions>().await?;
            indexer.concurrent_pipeline::<TxAffectedObjects>().await?;
            indexer.concurrent_pipeline::<TxBalanceChanges>().await?;
            indexer.sequential_pipeline::<SumObjTypes>().await?;

            let h_indexer = indexer.run().await.context("Failed to start indexer")?;

            cancel.cancelled().await;
            let _ = h_indexer.await;
        }
        Command::ResetDatabase { skip_migrations } => {
            reset_database(args.db_config, skip_migrations).await?;
        }
    }

    Ok(())
}
