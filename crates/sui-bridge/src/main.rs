// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use fastcrypto::traits::KeyPair;
use mysten_metrics::start_prometheus_server;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use sui_bridge::config::BridgeNodeConfig;
use sui_bridge::metrics::start_metrics_push_task;
use sui_bridge::node::run_bridge_node;
use sui_bridge::server::BridgeNodePublicMetadata;
use sui_config::Config;
use tracing::info;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = BridgeNodeConfig::load(&args.config_path).unwrap();

    // Init metrics server
    let metrics_address =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), config.metrics_port);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    info!("Metrics server started at port {}", config.metrics_port);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    let metadata = BridgeNodePublicMetadata::new(VERSION, config.metrics_key_pair.public().clone());

    start_metrics_push_task(
        &config.metrics,
        config.metrics_key_pair.copy(),
        registry_service.clone(),
    );
    Ok(run_bridge_node(config, metadata, prometheus_registry)
        .await?
        .await?)
}
