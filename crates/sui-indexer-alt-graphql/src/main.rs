// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_indexer_alt_graphql::{
    args::{Args, Command},
    start_rpc,
};
use tokio::signal;
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
        Command::Rpc { rpc_args } => {
            let cancel = CancellationToken::new();

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

            let h_rpc = start_rpc(rpc_args, cancel.child_token()).await?;

            let _ = h_rpc.await;
            cancel.cancel();
            let _ = h_ctrl_c.await;
        }
    }

    Ok(())
}
