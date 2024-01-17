// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use mysten_enoki_gas_pool::command::Command;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::info;

#[tokio::main]
async fn main() {
    let command = Command::parse();
    let metric_address = command
        .get_metrics_port()
        .map(|port| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port));
    let prometheus_registry = if let Some(metric_address) = metric_address {
        let registry_service = mysten_metrics::start_prometheus_server(metric_address);
        let prometheus_registry = registry_service.default_registry();
        Some(prometheus_registry)
    } else {
        None
    };

    let mut config = telemetry_subscribers::TelemetryConfig::new()
        .with_log_level("off,sui_gas_station=info")
        .with_env();
    if let Some(prometheus_registry) = &prometheus_registry {
        config = config.with_prom_registry(prometheus_registry);
    }
    let _guard = config.init();
    if let Some(metric_address) = metric_address {
        info!("Metrics server started at {:?}", metric_address);
    }

    command.execute(prometheus_registry).await;
}
