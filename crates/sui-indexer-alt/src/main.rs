// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::{
    args::Args,
    handlers::{
        ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
        kv_objects::KvObjects, kv_transactions::KvTransactions,
        tx_affected_addresses::TxAffectedAddress, tx_affected_objects::TxAffectedObjects,
        tx_balance_changes::TxBalanceChanges, tx_calls_fun::TxCallsFun, tx_digests::TxDigests,
        tx_kinds::TxKinds,
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

    let mut indexer = Indexer::new(args.indexer_config, cancel.clone()).await?;

    indexer.concurrent_pipeline::<EvEmitMod>().await?;
    indexer.concurrent_pipeline::<EvStructInst>().await?;
    indexer.concurrent_pipeline::<KvCheckpoints>().await?;
    indexer.concurrent_pipeline::<KvObjects>().await?;
    indexer.concurrent_pipeline::<KvTransactions>().await?;
    indexer.concurrent_pipeline::<TxAffectedAddress>().await?;
    indexer.concurrent_pipeline::<TxAffectedObjects>().await?;
    indexer.concurrent_pipeline::<TxBalanceChanges>().await?;
    indexer.concurrent_pipeline::<TxCallsFun>().await?;
    indexer.concurrent_pipeline::<TxDigests>().await?;
    indexer.concurrent_pipeline::<TxKinds>().await?;
    indexer.concurrent_pipeline::<TxKinds>().await?;

    let h_indexer = indexer.run().await.context("Failed to start indexer")?;

    cancel.cancelled().await;
    let _ = h_indexer.await;

    Ok(())
}
