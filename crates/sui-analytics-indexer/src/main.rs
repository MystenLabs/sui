// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use prometheus::Registry;
use std::env;
use sui_analytics_indexer::metrics::Metrics;
use sui_analytics_indexer::{IndexerConfig, build_analytics_indexer};
use sui_futures::service::Error;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args: Vec<String> = env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");

    let config: IndexerConfig = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;
    info!("Parsed config: {:#?}", config);

    let registry_service = mysten_metrics::start_prometheus_server(
        format!(
            "{}:{}",
            config.client_metric_host, config.client_metric_port
        )
        .parse()
        .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();

    let is_bounded_job = config.last_checkpoint.is_some();

    let metrics = Metrics::new(&registry);

    let service = build_analytics_indexer(config, metrics, registry).await?;

    match service.main().await {
        Ok(()) => {
            info!("Indexer completed successfully");
        }
        Err(Error::Terminated) => {
            info!("Received termination signal");
            if is_bounded_job {
                std::process::exit(1);
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }

    Ok(())
}
