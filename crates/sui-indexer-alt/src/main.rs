// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt::args::Args;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::db::reset_database;
use sui_indexer_alt::start_indexer;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Indexer {
            indexer,
            consistency_config,
        } => {
            start_indexer(indexer, args.db_config, consistency_config, true).await?;
        }
        Command::ResetDatabase { skip_migrations } => {
            reset_database(args.db_config, skip_migrations).await?;
        }
        #[cfg(feature = "benchmark")]
        Command::Benchmark { config } => {
            sui_indexer_alt::benchmark::run_benchmark(config, args.db_config).await?;
        }
    }

    Ok(())
}
