// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use sui_indexer_alt_graphql::{
    args::{Args, Command},
    start_rpc,
};
use sui_indexer_alt_metrics::MetricsService;
use tokio::signal;
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
        Command::Rpc {
            rpc_args,
            metrics_args,
        } => {
            let cancel = CancellationToken::new();

            let registry = Registry::new_custom(Some("graphql_alt".into()), None)
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

            let h_rpc =
                start_rpc(rpc_args, VERSION, metrics.registry(), cancel.child_token()).await?;

            let h_metrics = metrics.run().await?;

            let _ = h_rpc.await;
            cancel.cancel();
            let _ = h_metrics.await;
            let _ = h_ctrl_c.await;
        }
    }

    Ok(())
}
