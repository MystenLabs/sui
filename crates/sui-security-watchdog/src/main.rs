// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;

use anyhow::Result;
use clap::*;
use std::env;
use sui_security_watchdog::scheduler::SchedulerService;
use sui_security_watchdog::SecurityWatchdogConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    env_logger::init();
    let config = SecurityWatchdogConfig::parse();
    let pd_api_key = env::var("PD_API_KEY").expect("PD_API_KEY env var must be set");
    let sf_password = env::var("SF_PASSWORD").expect("SF_PASSWORD env var must be set");

    let registry_service = mysten_metrics::start_prometheus_server(
        format!(
            "{}:{}",
            config.client_metric_host, config.client_metric_port
        )
        .parse()
        .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    registry
        .register(mysten_metrics::uptime_metric(
            "security-watchdog",
            "v0",
            "N/A",
        ))
        .unwrap();
    let service = SchedulerService::new(&config, &registry, pd_api_key, sf_password).await?;
    service.schedule().await?;
    service.start().await?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
