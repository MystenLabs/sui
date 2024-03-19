// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use prometheus::Registry;
use sui_analytics_indexer::{
    analytics_metrics::AnalyticsMetrics, errors::AnalyticsIndexerError, make_analytics_processor,
    AnalyticsIndexerConfig,
};
use sui_indexer::framework::IndexerBuilder;
use sui_indexer::metrics::IndexerMetrics;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), AnalyticsIndexerError> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = AnalyticsIndexerConfig::parse();
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
    mysten_metrics::init_metrics(&registry);
    let metrics = AnalyticsMetrics::new(&registry);
    let indexer_metrics = IndexerMetrics::new(&registry);

    let rest_url = config.rest_url.clone();
    let processor = make_analytics_processor(config, metrics)
        .await
        .map_err(|e| AnalyticsIndexerError::GenericError(e.to_string()))?;
    IndexerBuilder::new(indexer_metrics)
        .last_downloaded_checkpoint(processor.last_committed_checkpoint())
        .rest_url(&rest_url)
        .handler(processor)
        .run()
        .await;
    Ok(())
}
