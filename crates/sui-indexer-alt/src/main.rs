// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::benchmark::run_indexer_benchmark;
use sui_indexer_alt::db::reset_database;
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

    match args.command {
        Command::Indexer(indexer_config) => {
            let mut indexer = Indexer::new(args.db_config, indexer_config, cancel.clone()).await?;

            indexer.register_pipelines().await?;

            let h_indexer = indexer.run().await.context("Failed to start indexer")?;

            cancel.cancelled().await;
            let _ = h_indexer.await;
        }
        Command::ResetDatabase { skip_migrations } => {
            reset_database(args.db_config, skip_migrations).await?;
        }
        Command::Benchmark(bench_config) => {
            run_indexer_benchmark(args.db_config, bench_config).await?;
        }
    }

    Ok(())
}
