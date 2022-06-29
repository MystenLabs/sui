// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use sui_config::sui_config_dir;
use sui_config::SUI_GATEWAY_CONFIG;
use sui_core::gateway_state::GatewayMetrics;
use sui_gateway::create_client;
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::gateway_api::{GatewayReadApiImpl, TransactionBuilderImpl};
use sui_json_rpc::gateway_api::{GatewayWalletSyncApiImpl, RpcGatewayImpl};
use sui_json_rpc::JsonRpcServerBuilder;
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
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let options: RpcGatewayOpt = RpcGatewayOpt::parse();
    let config_path = options
        .config
        .unwrap_or(sui_config_dir()?.join(SUI_GATEWAY_CONFIG));
    info!(?config_path, "Gateway config file path");

    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", prom_binding);
    let prometheus_registry = sui_node::metrics::start_prometheus_server(prom_binding);

    let metrics = GatewayMetrics::new(&prometheus_registry);
    let client = create_client(&config_path, metrics)?;

    let address = SocketAddr::new(IpAddr::V4(options.host), options.port);
    let mut server = JsonRpcServerBuilder::new(false, &prometheus_registry)?;
    server.register_module(RpcGatewayImpl::new(client.clone()))?;
    server.register_module(GatewayReadApiImpl::new(client.clone()))?;
    server.register_module(TransactionBuilderImpl::new(client.clone()))?;
    server.register_module(BcsApiImpl::new_with_gateway(client.clone()))?;
    server.register_module(GatewayWalletSyncApiImpl::new(client))?;

    let server_handle = server
        .start(address)
        .await?
        .into_http_server_handle()
        .expect("Expect a http server handle here");

    server_handle.await;
    Ok(())
}
