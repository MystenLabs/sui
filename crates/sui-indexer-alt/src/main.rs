// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use sui_indexer_alt::args::Args;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::db::reset_database;
use sui_indexer_alt::start_indexer;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Indexer {
            ingestion_args,
            indexer_args,
            config,
        } => {
            let config_contents = fs::read_to_string(config)
                .await
                .context("failed to read configuration TOML file")?;

            let indexer_config: IndexerConfig = toml::from_str(&config_contents)
                .context("Failed to parse configuration TOML file.")?;

            start_indexer(
                args.db_args,
                indexer_args,
                ingestion_args,
                indexer_config,
                true,
            )
            .await?;
        }

        Command::GenerateConfig => {
            let config = IndexerConfig::default();
            let config_toml = toml::to_string_pretty(&config)
                .context("Failed to serialize default configuration to TOML.")?;

            println!("{}", config_toml);
        }

        Command::ResetDatabase { skip_migrations } => {
            reset_database(args.db_args, skip_migrations).await?;
        }

        #[cfg(feature = "benchmark")]
        Command::Benchmark {
            benchmark_args,
            config,
        } => {
            let config_contents = fs::read_to_string(config)
                .await
                .context("failed to read configuration TOML file")?;

            let indexer_config: IndexerConfig = toml::from_str(&config_contents)
                .context("Failed to parse configuration TOML file.")?;

            sui_indexer_alt::benchmark::run_benchmark(args.db_args, benchmark_args, indexer_config)
                .await?;
        }
    }

    Ok(())
}
