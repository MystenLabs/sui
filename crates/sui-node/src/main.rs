// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use multiaddr::Multiaddr;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::{Config, NodeConfig};
use sui_node::metrics;
use sui_telemetry::send_telemetry_event;
use tokio::task;
use tokio::time::sleep;
use tracing::info;

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-", env!("GIT_REVISION"));

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config_path)?;

    let registry_service = metrics::start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();

    info!("Sui Node version: {VERSION}");

    info!(
        "Started Prometheus HTTP endpoint at {}",
        config.metrics_address
    );

    // Initialize logging
    let (_guard, filter_handle) =
        telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
            .with_env()
            .with_prom_registry(&prometheus_registry)
            .init();

    if let Some(listen_address) = args.listen_address {
        config.network_address = listen_address;
    }

    let is_validator = config.consensus_config().is_some();
    task::spawn(async move {
        loop {
            sleep(Duration::from_secs(3600)).await;
            send_telemetry_event(is_validator).await;
        }
    });

    sui_node::admin::start_admin_server(config.admin_interface_port, filter_handle);

    let _node = sui_node::SuiNode::start(&config, registry_service).await?;
    // TODO: Do we want to provide a way for the node to gracefully shutdown?
    loop {
        tokio::time::sleep(Duration::from_secs(1000)).await;
    }
}
