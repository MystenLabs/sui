// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use tracing::info;

use clap::Parser;

use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::wallet_context::WalletContext;
use telemetry_subscribers::TelemetryConfig;

use sui_source_validation_service::{host_port, initialize, parse_config, serve, AppState};

#[derive(Parser, Debug)]
struct Args {
    config_path: PathBuf,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _logging_guard = TelemetryConfig::new().with_env().init();
    let package_config = parse_config(args.config_path)?;
    let sui_config = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let context = WalletContext::new(&sui_config, None, None).await?;
    let tmp_dir = tempfile::tempdir()?;
    let sources = initialize(&context, &package_config, tmp_dir.path()).await?;
    info!("verification complete");
    info!("serving on {}", host_port());
    serve(AppState { sources })?
        .await
        .map_err(anyhow::Error::from)
}
