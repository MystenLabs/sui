// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};
use tracing::info;

use clap::Parser;

use telemetry_subscribers::TelemetryConfig;

use sui_source_validation_service::{
    host_port, initialize, /*listen_for_upgrades,*/ parse_config, serve, upgrade_listener,
    AppState,
};

#[derive(Parser, Debug)]
struct Args {
    config_path: PathBuf,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _logging_guard = TelemetryConfig::new().with_env().init();
    let package_config = parse_config(args.config_path)?;
    let tmp_dir = tempfile::tempdir()?;
    let start = tokio::time::Instant::now();
    // let sources = BTreeMap::new();
    let sources = initialize(&package_config, tmp_dir.path()).await?;
    info!("verification complete in {:?}", start.elapsed());
    // tokio::spawn(async move { listen_for_upgrades().await });
    let copy = sources.clone(); // FIXME
    tokio::spawn(async move { upgrade_listener(&copy).await });
    info!("serving on {}", host_port());
    serve(AppState { sources })?
        .await
        .map_err(anyhow::Error::from)
}
