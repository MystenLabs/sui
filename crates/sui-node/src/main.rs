// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::{Config, NodeConfig};
use sui_node::metrics;
use sui_protocol_config::SupportedProtocolVersions;
use sui_telemetry::send_telemetry_event;
use sui_types::multiaddr::Multiaddr;
use tokio::task;
use tokio::time::sleep;
use tracing::info;

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        let version = git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = ""
        );

        if version.is_empty() {
            panic!("unable to query git revision");
        }
        version
    }
};
const VERSION: &str = const_str::concat!(env!("CARGO_PKG_VERSION"), "-", GIT_REVISION);

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Ensure that a validator never calls get_for_min_version/get_for_max_version.
    // TODO: re-enable after we figure out how to eliminate crashes in prod because of this.
    // ProtocolConfig::poison_get_for_min_version();

    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config_path)?;
    assert!(
        config.supported_protocol_versions.is_none(),
        "supported_protocol_versions cannot be read from the config file"
    );
    config.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);

    let registry_service = metrics::start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();
    prometheus_registry
        .register(mysten_metrics::uptime_metric(VERSION))
        .unwrap();

    // Initialize logging
    let (_guard, filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    info!("Sui Node version: {VERSION}");
    info!(
        "Supported protocol versions: {:?}",
        config.supported_protocol_versions
    );

    info!(
        "Started Prometheus HTTP endpoint at {}",
        config.metrics_address
    );

    metrics::start_metrics_push_task(&config, registry_service.clone());

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

    let node = sui_node::SuiNode::start(&config, registry_service).await?;
    sui_node::admin::start_admin_server(node.clone(), config.admin_interface_port, filter_handle);

    // TODO: Do we want to provide a way for the node to gracefully shutdown?
    loop {
        tokio::time::sleep(Duration::from_secs(1000)).await;
    }
}
