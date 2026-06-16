// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;
use futures::TryFutureExt as _;
use prometheus::Registry;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_metrics::uptime;
use sui_rpc_node::args::Args;
use sui_rpc_node::args::Command;
use sui_rpc_node::config::ServiceConfig;
use sui_rpc_node::start_restorer;
use sui_rpc_node::start_serve;
use sui_rpc_node::start_service;
use tokio::fs;

bin_version::git_revision!();

const BIN_NAME: &str = "sui-rpc-node";

static VERSION: &str = const_str::concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
    ".",
    env!("CARGO_PKG_VERSION_PATCH"),
    "-",
    GIT_REVISION,
);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Run {
            database_path,
            indexer_args,
            client_args,
            metrics_args,
            config,
        } => {
            let config = read_config(config).await?;
            let registry = build_registry()?;
            let metrics = MetricsService::new(metrics_args, registry);
            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let s_service = start_service(
                database_path,
                indexer_args,
                client_args,
                BIN_NAME,
                VERSION,
                config,
                metrics.registry(),
            )
            .await?;
            let s_metrics = metrics.run().await?;

            match s_service.attach(s_metrics).main().await {
                Ok(()) | Err(Error::Terminated) => {}
                Err(Error::Aborted) => std::process::exit(1),
                Err(Error::Task(_)) => std::process::exit(2),
            }
        }

        Command::Restore {
            database_path,
            formal_snapshot_args,
            storage_connection_args,
            restore_args,
            metrics_args,
            config,
        } => {
            // Only the `db` and `restore` sections are consulted
            // during a restore; the rest of the service config is
            // irrelevant to it.
            let config = read_config(config).await?;
            // The `--shard-concurrency` CLI flag overrides the config.
            let shard_concurrency = restore_args
                .shard_concurrency
                .unwrap_or(config.restore.shard_concurrency);
            let registry = build_registry()?;
            let metrics = MetricsService::new(metrics_args, registry);
            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let (s_restorer, finalizer) = start_restorer(
                database_path,
                formal_snapshot_args,
                storage_connection_args,
                shard_concurrency,
                config.db.to_db_options(),
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
                // The post-restore finalize must run only after
                // the restore drove every shard to completion.
                // Terminating or aborting mid-restore voids that
                // guarantee, so we exit nonzero rather than
                // claim success.
                Err(Error::Terminated | Error::Aborted) => std::process::exit(1),
                Err(Error::Task(_)) => std::process::exit(2),
            }
        }

        Command::Serve {
            database_path,
            metrics_args,
            config,
        } => {
            let config = read_config(config).await?;
            let registry = build_registry()?;
            let metrics = MetricsService::new(metrics_args, registry);
            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric")?;

            let s_service =
                start_serve(database_path, BIN_NAME, VERSION, config, metrics.registry()).await?;
            let s_metrics = metrics.run().await?;

            match s_service.attach(s_metrics).main().await {
                Ok(()) | Err(Error::Terminated) => {}
                Err(Error::Aborted) => std::process::exit(1),
                Err(Error::Task(_)) => std::process::exit(2),
            }
        }

        Command::GenerateConfig => {
            let cfg = ServiceConfig::example();
            let toml = toml::to_string_pretty(&cfg)
                .context("Failed to serialize default ServiceConfig to TOML")?;
            println!("{toml}");
        }
    }

    Ok(())
}

fn build_registry() -> anyhow::Result<Registry> {
    Registry::new_custom(Some("rpc_node".into()), None)
        .context("Failed to create Prometheus registry")
}

async fn read_config(path: Option<PathBuf>) -> anyhow::Result<ServiceConfig> {
    if let Some(path) = path {
        let contents = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read configuration TOML at {}", path.display()))?;
        toml::from_str(&contents).context("Failed to parse configuration TOML")
    } else {
        Ok(ServiceConfig::default())
    }
}
