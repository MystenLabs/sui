// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use prometheus::Registry;
use sui_indexer_alt::args::Args;
use sui_indexer_alt::args::Command;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::config::Merge;
use sui_indexer_alt::setup_indexer;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_schema::MIGRATIONS;
use sui_pg_db::reset_database;
use tokio::fs;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Indexer {
            client_args,
            indexer_args,
            metrics_args,
            config,
        } => {
            let indexer_config = read_config(&config).await?;
            info!("Starting indexer with config: {:?}", indexer_config);

            let cancel = CancellationToken::new();

            let registry = Registry::new_custom(Some("indexer_alt".into()), None)
                .context("Failed to create Prometheus registry.")?;

            let metrics = MetricsService::new(metrics_args, registry, cancel.child_token());

            let h_indexer = setup_indexer(
                args.db_args,
                indexer_args,
                client_args,
                indexer_config,
                true,
                metrics.registry(),
                cancel.child_token(),
            )
            .await?
            .run()
            .await
            .context("Failed to start indexer")?;

            let h_metrics = metrics.run().await?;

            // Wait for the indexer to finish, then force the supporting services to shut down
            // using the cancellation token.
            let _ = h_indexer.await;
            cancel.cancel();
            let _ = h_metrics.await;
        }

        Command::GenerateConfig => {
            let config = IndexerConfig::example();
            let config_toml = toml::to_string_pretty(&config)
                .context("Failed to serialize default configuration to TOML.")?;

            println!("{config_toml}");
        }

        Command::MergeConfigs { config } => {
            let mut files = config.into_iter();

            let Some(file) = files.next() else {
                bail!("At least one configuration file must be provided.");
            };

            let mut indexer_config = read_config(&file).await?;
            for file in files {
                indexer_config =
                    indexer_config.merge(read_config(&file).await.with_context(|| {
                        format!("Failed to read configuration file: {}", file.display())
                    })?);
            }

            let config_toml = toml::to_string_pretty(&indexer_config)
                .context("Failed to serialize merged configuration to TOML.")?;

            println!("{config_toml}");
        }

        Command::ResetDatabase { skip_migrations } => {
            reset_database(
                args.db_args,
                (!skip_migrations).then(|| Indexer::migrations(Some(&MIGRATIONS))),
            )
            .await?;
        }

        #[cfg(feature = "benchmark")]
        Command::Benchmark {
            benchmark_args,
            config,
        } => {
            let indexer_config = read_config(&config).await?;
            sui_indexer_alt::benchmark::run_benchmark(args.db_args, benchmark_args, indexer_config)
                .await?;
        }
    }

    Ok(())
}

async fn read_config(path: &Path) -> Result<IndexerConfig> {
    let config_contents = fs::read_to_string(path)
        .await
        .context("Failed to read configuration TOML file")?;

    toml::from_str(&config_contents).context("Failed to parse configuration TOML file.")
}
