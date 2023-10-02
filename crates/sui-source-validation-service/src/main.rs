// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::info;

use clap::Parser;

use telemetry_subscribers::TelemetryConfig;

use sui_source_validation_service::{
    host_port, initialize, parse_config, serve, watch_for_upgrades, AppState,
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
    let sources = initialize(&package_config, tmp_dir.path()).await?;
    let app_state = Arc::new(RwLock::new(AppState { sources }));
    let app_state_clone = app_state.clone();
    let mut threads = vec![];
    let watcher = tokio::spawn(async move { watch_for_upgrades(&package_config, app_state).await });
    threads.push(watcher);
    info!("verification complete in {:?}", start.elapsed());
    info!("serving on {}", host_port());
    let server = tokio::spawn(async { serve(app_state_clone)?.await.map_err(anyhow::Error::from) });
    threads.push(server);
    for t in threads {
        t.await.unwrap()?;
    }
    Ok(())
}
