// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use sui_indexer_alt_consistent_store::{
    args::{Args, Command},
    config::ServiceConfig,
    restore::start_restorer,
    start_service,
};
use sui_indexer_alt_metrics::{uptime, MetricsService};
use tokio::{fs, signal, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

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
            let cancel = CancellationToken::new();
            let registry = Registry::new_custom(Some("consistent_store".into()), None)
                .context("Failed to create Prometheus registry")?;

            let metrics = MetricsService::new(metrics_args, registry, cancel.child_token());
            let h_ctrl_c = handle_interrupt(cancel.clone());

            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let h_service = start_service(
                database_path,
                indexer_args,
                client_args,
                rpc_args,
                VERSION,
                config,
                metrics.registry(),
                cancel.child_token(),
            )
            .await?;

            let h_metrics = metrics.run().await?;

            let _ = h_service.await;
            cancel.cancel();
            let _ = h_metrics.await;
            let _ = h_ctrl_c.await;
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
            let cancel = CancellationToken::new();
            let registry = Registry::new_custom(Some("consistent_store".into()), None)
                .context("Failed to create Prometheus registry")?;

            let metrics = MetricsService::new(metrics_args, registry, cancel.child_token());
            let h_ctrl_c = handle_interrupt(cancel.clone());

            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let h_restorer = start_restorer(
                database_path,
                formal_snapshot_args,
                storage_connection_args,
                restore_args,
                pipeline.into_iter().collect(),
                config.rocksdb,
                metrics.registry(),
                cancel.child_token(),
            )
            .await?;

            let h_metrics = metrics.run().await?;

            let _ = h_restorer.await;
            cancel.cancel();
            let _ = h_metrics.await;
            let _ = h_ctrl_c.await;
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

fn handle_interrupt(cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::select! {
            _ = cancel.cancelled() => {}
            _ = signal::ctrl_c() => {
                info!("Received Ctrl-C, shutting down...");
                cancel.cancel();
            }
        }
    })
}
