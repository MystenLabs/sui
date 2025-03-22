// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use sui_indexer_alt_jsonrpc::{
    args::{Args, Command},
    config::RpcLayer,
    start_rpc,
};
use sui_indexer_alt_metrics::MetricsService;
use tokio::{fs, signal};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    match args.command {
        Command::Rpc {
            database_url,
            db_args,
            rpc_args,
            system_package_task_args,
            metrics_args,
            node_args,
            config,
        } => {
            let rpc_config = if let Some(path) = config {
                let contents = fs::read_to_string(path)
                    .await
                    .context("Failed to read configuration TOML file")?;

                toml::from_str(&contents).context("Failed to parse configuration TOML file")?
            } else {
                RpcLayer::default()
            }
            .finish();

            let cancel = CancellationToken::new();

            let registry = Registry::new_custom(Some("jsonrpc_alt".into()), None)
                .context("Failed to create Prometheus registry.")?;

            let metrics = MetricsService::new(metrics_args, registry, cancel.child_token());

            let h_ctrl_c = tokio::spawn({
                let cancel = cancel.clone();
                async move {
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = signal::ctrl_c() => {
                            info!("Received Ctrl-C, shutting down...");
                            cancel.cancel();
                        }
                    }
                }
            });

            let h_rpc = start_rpc(
                Some(database_url),
                db_args,
                rpc_args,
                node_args,
                system_package_task_args,
                rpc_config,
                metrics.registry(),
                cancel.child_token(),
            )
            .await?;

            let h_metrics = metrics.run().await?;

            let _ = h_rpc.await;
            cancel.cancel();
            let _ = h_metrics.await;
            let _ = h_ctrl_c.await;
        }

        Command::GenerateConfig => {
            let config = RpcLayer::example();
            let config_toml = toml::to_string_pretty(&config)
                .context("Failed to serialize default configuration to TOML.")?;

            println!("{config_toml}");
        }
    }

    Ok(())
}
