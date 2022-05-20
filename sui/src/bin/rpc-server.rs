// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use jsonrpsee::{
    http_server::{AccessControlBuilder, HttpServerBuilder},
    RpcModule,
};
use jsonrpsee_core::middleware::Middleware;
use prometheus_exporter::prometheus::{
    register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec,
};
use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Instant,
};
use sui::{
    api::{RpcGatewayOpenRpc, RpcGatewayServer},
    config::sui_config_dir,
    rpc_gateway::RpcGatewayImpl,
};
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
        .unwrap_or(sui_config_dir()?.join("gateway.conf"));
    info!(?config_path, "Gateway config file path");

    let server_builder = HttpServerBuilder::default();
    let mut ac_builder = AccessControlBuilder::default();

    if let Ok(value) = env::var("ACCESS_CONTROL_ALLOW_ORIGIN") {
        let list = value.split(',').collect::<Vec<_>>();
        info!("Setting ACCESS_CONTROL_ALLOW_ORIGIN to : {:?}", list);
        ac_builder = ac_builder.set_allowed_origins(list)?;
    }

    let acl = ac_builder.build();
    info!(?acl);

    let server = server_builder
        .set_access_control(acl)
        .set_middleware(JsonRpcMetrics::new())
        .build(SocketAddr::new(IpAddr::V4(options.host), options.port))
        .await?;

    let mut module = RpcModule::new(());
    let open_rpc = RpcGatewayOpenRpc::open_rpc();
    module.register_method("rpc.discover", move |_, _| Ok(open_rpc.clone()))?;
    module.merge(RpcGatewayImpl::new(&config_path)?.into_rpc())?;

    info!(
        "Available JSON-RPC methods : {:?}",
        module.method_names().collect::<Vec<_>>()
    );

    let addr = server.local_addr()?;
    let server_handle = server.start(module)?;
    info!(local_addr =? addr, "Sui RPC Gateway listening on local_addr");

    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", PROM_PORT_ADDR);
    prometheus_exporter::start(prom_binding).expect("Failed to start Prometheus exporter");

    server_handle.await;
    Ok(())
}

#[derive(Clone)]
struct JsonRpcMetrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
    /// Failed requests by route
    errors_by_route: IntCounterVec,
}

impl JsonRpcMetrics {
    pub fn new() -> Self {
        Self {
            requests_by_route: register_int_counter_vec!(
                "rpc_requests_by_route",
                "Number of requests by route",
                &["route"]
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec!(
                "req_latency_by_route",
                "Latency of a request by route",
                &["route"]
            )
            .unwrap(),
            errors_by_route: register_int_counter_vec!(
                "errors_by_route",
                "Number of errors by route",
                &["route"]
            )
            .unwrap(),
        }
    }
}

impl Middleware for JsonRpcMetrics {
    type Instant = Instant;

    fn on_request(&self) -> Instant {
        Instant::now()
    }

    fn on_result(&self, name: &str, success: bool, started_at: Instant) {
        self.requests_by_route.with_label_values(&[name]).inc();
        let req_latency_secs = (Instant::now() - started_at).as_secs_f64();
        self.req_latency_by_route
            .with_label_values(&[name])
            .observe(req_latency_secs);
        if !success {
            self.errors_by_route.with_label_values(&[name]).inc();
        }
    }
}
