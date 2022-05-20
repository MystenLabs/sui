// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use multiaddr::Multiaddr;
use std::path::PathBuf;
use sui_config::{NodeConfig, PersistedConfig};
use tracing::info;

const PROM_PORT_ADDR: &str = "0.0.0.0:9184";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "sui-node".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    };

    let _guard = telemetry_subscribers::init(config);

    let args = Args::parse();

    let mut config = PersistedConfig::<NodeConfig>::read(&args.config_path)?;

    // TODO: Switch from prometheus exporter. See https://github.com/MystenLabs/sui/issues/1907
    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", PROM_PORT_ADDR);
    prometheus_exporter::start(prom_binding).expect("Failed to start Prometheus exporter");

    if let Some(listen_address) = args.listen_address {
        config.network_address = listen_address;
    }

    sui_node::SuiNode::start(&config).await?;

    Ok(())
}
