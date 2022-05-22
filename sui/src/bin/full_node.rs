// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use sui::{
    config::{sui_config_dir, FULL_NODE_DB_PATH},
    sui_full_node::SuiFullNode,
};
use sui_gateway::api::{RpcGatewayOpenRpc, RpcGatewayServer};
use sui_gateway::json_rpc::JsonRpcServerBuilder;
use tracing::info;

const DEFAULT_NODE_SERVER_PORT: &str = "5002";
const DEFAULT_NODE_SERVER_ADDR_IPV4: &str = "127.0.0.1";

#[derive(Parser)]
#[clap(name = "Sui Full Node", about = "TODO", rename_all = "kebab-case")]
struct SuiNodeOpt {
    #[clap(long)]
    db_path: Option<String>,

    #[clap(long)]
    config: Option<PathBuf>,

    #[clap(long, default_value = DEFAULT_NODE_SERVER_PORT)]
    port: u16,

    #[clap(long, default_value = DEFAULT_NODE_SERVER_ADDR_IPV4)]
    host: Ipv4Addr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "sui_node".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    };
    #[allow(unused)]
    let guard = telemetry_subscribers::init(config);

    let options: SuiNodeOpt = SuiNodeOpt::parse();
    let db_path = options
        .db_path
        .map(PathBuf::from)
        .unwrap_or(sui_config_dir()?.join(FULL_NODE_DB_PATH));

    let config_path = options
        .config
        .unwrap_or(sui_config_dir()?.join("network.conf"));
    info!("Node config file path: {:?}", config_path);

    let address = SocketAddr::new(IpAddr::V4(options.host), options.port);
    let mut server = JsonRpcServerBuilder::new()?;
    server.register_open_rpc(RpcGatewayOpenRpc::open_rpc())?;
    server.register_methods(
        SuiFullNode::start_with_genesis(&config_path, &db_path)
            .await?
            .into_rpc(),
    )?;

    let server_handle = server.start(address).await?;

    server_handle.await;
    Ok(())
}
