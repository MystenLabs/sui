// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::{args::Args, Indexer};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let cancel = CancellationToken::new();

    let indexer = Indexer::new(args.indexer_config, cancel.clone()).await?;

    let h_indexer = indexer.run().await.context("Failed to start indexer")?;

    cancel.cancelled().await;
    let _ = h_indexer.await;

    Ok(())
}
