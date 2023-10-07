// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use prometheus::Registry;
use std::sync::Arc;
use sui_analytics_indexer::errors::AnalyticsIndexerError::GenericError;
use sui_analytics_indexer::{
    analytics_metrics::AnalyticsMetrics, errors::AnalyticsIndexerError, make_analytics_processor,
    AnalyticsIndexerConfig,
};
use sui_indexer::framework::IndexerBuilder;
use tokio::runtime;
use tracing::info;

fn main() -> Result<(), AnalyticsIndexerError> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = AnalyticsIndexerConfig::parse();
    info!("Parsed config: {:#?}", config);

    let rest_url = config.rest_url.clone();
    let fetcher_runtime = runtime::Builder::new_multi_thread()
        .thread_name("checkpoint-fetcher")
        .enable_all()
        .build()
        .expect("Failed to create runtime");
    let uploader_runtime = Arc::new(
        runtime::Builder::new_multi_thread()
            .thread_name("checkpoint-uploader")
            .enable_all()
            .build()
            .expect("Failed to create runtime"),
    );
    let processor = fetcher_runtime
        .block_on(make_analytics_processor(config, uploader_runtime))
        .map_err(|_| GenericError("dsad".to_string()))?;
    fetcher_runtime.block_on(
        IndexerBuilder::new()
            .last_downloaded_checkpoint(processor.last_committed_checkpoint())
            .rest_url(&rest_url)
            .handler(processor)
            .run(),
    );

    Ok(())
}
