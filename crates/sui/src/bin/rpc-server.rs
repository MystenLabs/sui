// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use sui::config::SUI_GATEWAY_CONFIG;
use sui_config::sui_config_dir;
use sui_gateway::rpc_gateway::{create_client, GatewayReadApiImpl, TransactionBuilderImpl};
use sui_gateway::{json_rpc::JsonRpcServerBuilder, rpc_gateway::RpcGatewayImpl};
use tracing::info;
const DEFAULT_RPC_SERVER_PORT: &str = "5001";
const DEFAULT_RPC_SERVER_ADDR_IPV4: &str = "127.0.0.1";
const PROM_PORT_ADDR: &str = "0.0.0.0:9184";

#[cfg(test)]
#[path = "../unit_tests/rpc_server_tests.rs"]
mod rpc_server_tests;

#[derive(Parser)]
#[clap(
    name = "Sui RPC Gateway",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct RpcGatewayOpt {
    #[clap(long)]
    config: Option<PathBuf>,

    #[clap(long, default_value = DEFAULT_RPC_SERVER_PORT)]
    port: u16,

    #[clap(long, default_value = DEFAULT_RPC_SERVER_ADDR_IPV4)]
    host: Ipv4Addr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "rpc_gateway".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    };
    #[allow(unused)]
    let guard = telemetry_subscribers::init(config);
    let options: RpcGatewayOpt = RpcGatewayOpt::parse();
    let config_path = options
        .config
        .unwrap_or(sui_config_dir()?.join(SUI_GATEWAY_CONFIG));
    info!(?config_path, "Gateway config file path");

    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", PROM_PORT_ADDR);
    prometheus_exporter::start(prom_binding).expect("Failed to start Prometheus exporter");

    let client = create_client(&config_path)?;

    let address = SocketAddr::new(IpAddr::V4(options.host), options.port);
    let mut server = JsonRpcServerBuilder::new()?;
    server.register_module(RpcGatewayImpl::new(client.clone()))?;
    server.register_module(GatewayReadApiImpl::new(client.clone()))?;
    server.register_module(TransactionBuilderImpl::new(client))?;

    let server_handle = server.start(address).await?;

    server_handle.await;
    Ok(())
}
