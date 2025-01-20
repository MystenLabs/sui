// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use sui_indexer_alt_jsonrpc::{args::Args, start_rpc};
use sui_indexer_alt_metrics::MetricsService;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args {
        db_args,
        rpc_args,
        metrics_args,
    } = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let cancel = CancellationToken::new();

    let registry = Registry::new_custom(Some("jsonrpc_alt".into()), None)
        .context("Failed to create Prometheus registry.")?;

    let metrics = MetricsService::new(metrics_args, registry, cancel.child_token());

    let h_rpc = start_rpc(db_args, rpc_args, metrics.registry(), cancel.child_token()).await?;
    let h_metrics = metrics.run().await?;

    let _ = h_rpc.await;
    cancel.cancel();
    let _ = h_metrics.await;
    Ok(())
}
