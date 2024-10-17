// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::{
    args::Args,
    handlers::{
        kv_checkpoints::KvCheckpoints, kv_objects::KvObjects, kv_transactions::KvTransactions,
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

    let mut indexer = Indexer::new(args.indexer_config, cancel.clone()).await?;

    indexer.pipeline::<KvCheckpoints>().await?;
    indexer.pipeline::<KvObjects>().await?;
    indexer.pipeline::<KvTransactions>().await?;
    indexer.pipeline::<TxAffectedObjects>().await?;
    indexer.pipeline::<TxBalanceChanges>().await?;

    let h_indexer = indexer.run().await.context("Failed to start indexer")?;

    cancel.cancelled().await;
    let _ = h_indexer.await;

    Ok(())
}
