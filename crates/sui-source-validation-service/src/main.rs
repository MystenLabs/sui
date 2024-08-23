// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::{path::PathBuf, sync::RwLock};
use tracing::info;

use clap::Parser;

use telemetry_subscribers::TelemetryConfig;

use sui_source_validation_service::{
    host_port, initialize, parse_config, serve, start_prometheus_server, watch_for_upgrades,
    AppState, DirectorySource, Network, PackageSource, RepositorySource, SourceServiceMetrics,
    METRICS_HOST_PORT,
};

#[derive(Parser, Debug)]
struct Args {
    config_path: PathBuf,
}

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _logging_guard = TelemetryConfig::new().with_env().init();
    let package_config = parse_config(args.config_path)?;
    let tmp_dir = tempfile::tempdir()?;
    let start = tokio::time::Instant::now();
    let (sources, sources_list) = initialize(&package_config, tmp_dir.path()).await?;
    info!("verification complete in {:?}", start.elapsed());

    let metrics_listener = std::net::TcpListener::bind(METRICS_HOST_PORT)?;
    let registry_service = start_prometheus_server(metrics_listener);
    let prometheus_registry = registry_service.default_registry();
    let metrics = SourceServiceMetrics::new(&prometheus_registry);

    let app_state = Arc::new(RwLock::new(AppState {
        sources,
        metrics: Some(metrics),
        sources_list,
    }));
    let mut threads = vec![];
    let networks_to_watch = vec![
        Network::Mainnet,
        Network::Testnet,
        Network::Devnet,
        Network::Localnet,
    ];
    // spawn a watcher thread for upgrades for each network
    for network in networks_to_watch {
        let app_state_copy = app_state.clone();
        let packages: Vec<_> = package_config
            .clone()
            .packages
            .into_iter()
            .filter(|p| {
                matches!(
                    p,
                    PackageSource::Repository(RepositorySource {
                        network: Some(n),
                        ..
                    }) | PackageSource::Directory(DirectorySource {
                        network: Some(n),
                        ..
                    }) if *n == network,
                )
            })
            .collect();
        if packages.is_empty() {
            continue;
        }
        let watcher = tokio::spawn(async move {
            watch_for_upgrades(packages, app_state_copy, network, None).await
        });
        threads.push(watcher);
    }

    let app_state_copy = app_state.clone();
    let server = tokio::spawn(async { serve(app_state_copy).await });
    threads.push(server);
    info!("serving on {}", host_port());
    for t in threads {
        t.await.unwrap()?;
    }
    Ok(())
}
