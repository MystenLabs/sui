// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::bootstrap::bootstrap;
use sui_indexer_alt::db::reset_database;
use sui_indexer_alt::handlers::kv_epoch_ends::KvEpochEnds;
use sui_indexer_alt::handlers::kv_epoch_starts::KvEpochStarts;
use sui_indexer_alt::handlers::kv_feature_flags::KvFeatureFlags;
use sui_indexer_alt::handlers::kv_protocol_configs::KvProtocolConfigs;
use sui_indexer_alt::pipeline::concurrent::PrunerConfig;
use sui_indexer_alt::{
    args::Args,
    handlers::{
        ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
        kv_objects::KvObjects, kv_transactions::KvTransactions, obj_versions::ObjVersions,
        sum_coin_balances::SumCoinBalances, sum_displays::SumDisplays, sum_obj_types::SumObjTypes,
        sum_packages::SumPackages, tx_affected_addresses::TxAffectedAddress,
        tx_affected_objects::TxAffectedObjects, tx_balance_changes::TxBalanceChanges,
        tx_calls::TxCalls, tx_digests::TxDigests, tx_kinds::TxKinds,
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
            consistent_pruning_interval,
            consistent_range: lag,
        } => {
            let retry_interval = indexer.ingestion_config.retry_interval;
            let mut indexer = Indexer::new(args.db_config, indexer, cancel.clone()).await?;

            let genesis = bootstrap(&indexer, retry_interval, cancel.clone()).await?;

            // Pipelines that rely on genesis information
            indexer
                .concurrent_pipeline(KvFeatureFlags(genesis.clone()), None)
                .await?;

            indexer
                .concurrent_pipeline(KvProtocolConfigs(genesis.clone()), None)
                .await?;

            // Pipelines that are split up into a summary table, and a write-ahead log, where the
            // write-ahead log needs to be pruned.
            let pruner_config = lag.map(|l| PrunerConfig {
                interval: consistent_pruning_interval,
                // Retain at least twice as much data as the lag, to guarantee overlap between the
                // summary table and the write-ahead log.
                retention: l * 2,
                // Prune roughly five minutes of data in one go.
                max_chunk_size: 5 * 300,
            });

            indexer.sequential_pipeline(SumCoinBalances, lag).await?;
            indexer
                .concurrent_pipeline(WalCoinBalances, pruner_config.clone())
                .await?;

            indexer.sequential_pipeline(SumObjTypes, lag).await?;
            indexer
                .concurrent_pipeline(WalObjTypes, pruner_config)
                .await?;

            // Other summary tables (without write-ahead log)
            indexer.sequential_pipeline(SumDisplays, None).await?;
            indexer.sequential_pipeline(SumPackages, None).await?;

            // Unpruned concurrent pipelines
            indexer.concurrent_pipeline(EvEmitMod, None).await?;
            indexer.concurrent_pipeline(EvStructInst, None).await?;
            indexer.concurrent_pipeline(KvCheckpoints, None).await?;
            indexer.concurrent_pipeline(KvEpochEnds, None).await?;
            indexer.concurrent_pipeline(KvEpochStarts, None).await?;
            indexer.concurrent_pipeline(KvObjects, None).await?;
            indexer.concurrent_pipeline(KvTransactions, None).await?;
            indexer.concurrent_pipeline(ObjVersions, None).await?;
            indexer.concurrent_pipeline(TxAffectedAddress, None).await?;
            indexer.concurrent_pipeline(TxAffectedObjects, None).await?;
            indexer.concurrent_pipeline(TxBalanceChanges, None).await?;
            indexer.concurrent_pipeline(TxCalls, None).await?;
            indexer.concurrent_pipeline(TxDigests, None).await?;
            indexer.concurrent_pipeline(TxKinds, None).await?;

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
