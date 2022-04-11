// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use jsonrpsee::http_server::HttpServerBuilder;
use jsonrpsee::RpcModule;
use tracing::info;

use clap::Parser;
use sui::rpc_gateway::RpcGatewayImpl;
use sui::rpc_gateway::RpcGatewayServer;
use sui::sui_config_dir;
const DEFAULT_REST_SERVER_PORT: &str = "5001";
const DEFAULT_REST_SERVER_ADDR_IPV4: &str = "127.0.0.1";

#[derive(Parser)]
#[clap(
    name = "Sui RPC Gateway",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct RpcGatewayOpt {
    #[clap(long)]
    config: Option<PathBuf>,

    #[clap(long, default_value = DEFAULT_REST_SERVER_PORT)]
    port: u16,

    #[clap(long, default_value = DEFAULT_REST_SERVER_ADDR_IPV4)]
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
        .unwrap_or(sui_config_dir()?.join("gateway.conf"));

    let server = HttpServerBuilder::default()
        .build(SocketAddr::new(IpAddr::V4(options.host), options.port))
        .await?;

    let mut module = RpcModule::new(());
    module.merge(RpcGatewayImpl::new(&config_path)?.into_rpc())?;

    let addr = server.local_addr()?;
    let server_handle = server.start(module)?;
    info!("Sui RPC Gateway listening on local_addr:{}", addr);

    server_handle.await;
    Ok(())
}
