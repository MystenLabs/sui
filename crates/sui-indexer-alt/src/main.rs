// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::bootstrap::bootstrap;
use sui_indexer_alt::db::reset_database;
use sui_indexer_alt::{
    args::Args,
    handlers::{
        ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
        kv_objects::KvObjects, kv_transactions::KvTransactions, obj_versions::ObjVersions,
        sum_coin_balances::SumCoinBalances, sum_displays::SumDisplays, sum_obj_types::SumObjTypes,
        sum_packages::SumPackages, tx_affected_addresses::TxAffectedAddress,
        tx_affected_objects::TxAffectedObjects, tx_balance_changes::TxBalanceChanges,
        tx_calls_fun::TxCallsFun, tx_digests::TxDigests, tx_kinds::TxKinds,
        wal_coin_balances::WalCoinBalances, wal_obj_types::WalObjTypes,
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
        Command::Indexer {
            indexer,
            consistent_range: lag,
        } => {
            let retry_interval = indexer.ingestion_config.retry_interval;
            let mut indexer = Indexer::new(args.db_config, indexer, cancel.clone()).await?;

            bootstrap(&indexer, retry_interval, cancel.clone()).await?;

            indexer.concurrent_pipeline::<EvEmitMod>().await?;
            indexer.concurrent_pipeline::<EvStructInst>().await?;
            indexer.concurrent_pipeline::<KvCheckpoints>().await?;
            indexer.concurrent_pipeline::<KvObjects>().await?;
            indexer.concurrent_pipeline::<KvTransactions>().await?;
            indexer.concurrent_pipeline::<ObjVersions>().await?;
            indexer.concurrent_pipeline::<TxAffectedAddress>().await?;
            indexer.concurrent_pipeline::<TxAffectedObjects>().await?;
            indexer.concurrent_pipeline::<TxBalanceChanges>().await?;
            indexer.concurrent_pipeline::<TxCallsFun>().await?;
            indexer.concurrent_pipeline::<TxDigests>().await?;
            indexer.concurrent_pipeline::<TxKinds>().await?;
            indexer.concurrent_pipeline::<TxKinds>().await?;
            indexer.concurrent_pipeline::<WalCoinBalances>().await?;
            indexer.concurrent_pipeline::<WalObjTypes>().await?;
            indexer.sequential_pipeline::<SumCoinBalances>(lag).await?;
            indexer.sequential_pipeline::<SumDisplays>(None).await?;
            indexer.sequential_pipeline::<SumObjTypes>(lag).await?;
            indexer.sequential_pipeline::<SumPackages>(None).await?;

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
