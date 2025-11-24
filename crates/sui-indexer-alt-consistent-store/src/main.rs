// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use futures::TryFutureExt as _;
use prometheus::Registry;
use sui_indexer_alt_consistent_store::{
    args::{Args, Command},
    config::ServiceConfig,
    restore::start_restorer,
    start_service,
};
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::{MetricsService, uptime};
use tokio::fs;

// Define the `GIT_REVISION` const
bin_version::git_revision!();

static VERSION: &str = const_str::concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
    ".",
    env!("CARGO_PKG_VERSION_PATCH"),
    "-",
    GIT_REVISION
);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Run {
            database_path,
            indexer_args,
            client_args,
            rpc_args,
            metrics_args,
            config,
        } => {
            let config = read_config(config).await?;
            let registry = Registry::new_custom(Some("consistent_store".into()), None)
                .context("Failed to create Prometheus registry")?;

            let metrics = MetricsService::new(metrics_args, registry);

            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let s_service = start_service(
                database_path,
                indexer_args,
                client_args,
                rpc_args,
                VERSION,
                config,
                metrics.registry(),
            )
            .await?;

            let s_metrics = metrics.run().await?;

            match s_service.attach(s_metrics).main().await {
                Ok(()) | Err(Error::Terminated) => {}

                Err(Error::Aborted) => {
                    std::process::exit(1);
                }

                Err(Error::Task(_)) => {
                    std::process::exit(2);
                }
            }
        }

        Command::Restore {
            database_path,
            formal_snapshot_args,
            storage_connection_args,
            restore_args,
            metrics_args,
            pipeline,
            config,
        } => {
            let config = read_config(config).await?;
            let registry = Registry::new_custom(Some("consistent_store".into()), None)
                .context("Failed to create Prometheus registry")?;

            let metrics = MetricsService::new(metrics_args, registry);

            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let (s_restorer, finalizer) = start_restorer(
                database_path,
                formal_snapshot_args,
                storage_connection_args,
                restore_args,
                pipeline.into_iter().collect(),
                config.rocksdb,
                metrics.registry(),
            )
            .await?;

            let s_metrics = metrics.run().await?;

            match s_restorer
                .attach(s_metrics)
                .main()
                .and_then(|_| finalizer.run().main())
                .await
            {
                Ok(()) => {}

                // We can only guarantee that the restorer succeeded if it is allowed to complete
                // without being instructed to exit or abort.
                Err(Error::Terminated | Error::Aborted) => {
                    std::process::exit(1);
                }

                Err(Error::Task(_)) => {
                    std::process::exit(2);
                }
            }
        }

        Command::GenerateConfig => {
            let config = ServiceConfig::example();
            let config_toml = toml::to_string_pretty(&config)
                .context("Failed to serialize default configuration to TOML.")?;

            println!("{config_toml}");
        }
    }

    Ok(())
}

async fn read_config(path: Option<PathBuf>) -> anyhow::Result<ServiceConfig> {
    if let Some(path) = path {
        let contents = fs::read_to_string(path)
            .await
            .context("Failed to read configuration TOML file")?;

        toml::from_str(&contents).context("Failed to parse configuration TOML file")
    } else {
        Ok(ServiceConfig::default())
    }
}
