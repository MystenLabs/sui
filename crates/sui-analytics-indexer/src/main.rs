// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use prometheus::Registry;
use std::env;
use sui_analytics_indexer::JobConfig;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args: Vec<String> = env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");

    // Parse the config
    let config: JobConfig = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;
    info!("Parsed config: {:#?}", config);

    info!("Using sui-indexer-alt-framework");
    run_with_alt_framework(config).await
}

async fn run_with_alt_framework(config: JobConfig) -> Result<()> {
    use std::time::Duration;
    use sui_analytics_indexer::indexer_alt::{AnalyticsIndexerConfig, start_analytics_indexer};

    // Setup metrics
    let registry_service = mysten_metrics::start_prometheus_server(
        format!(
            "{}:{}",
            config.client_metric_host, config.client_metric_port
        )
        .parse()
        .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();

    let cancel = tokio_util::sync::CancellationToken::new();

    // Create analytics indexer config from job config
    let analytics_config = AnalyticsIndexerConfig {
        job_config: config,
        write_concurrency: 10,
        watermark_interval: Duration::from_secs(60),
        first_checkpoint: None,
        last_checkpoint: None,
    };

    let mut h_indexer = start_analytics_indexer(analytics_config, registry, cancel.clone()).await?;

    enum ExitReason {
        Completed,
        UserInterrupt,
        Terminated,
    }

    let is_bounded_job = false; // TODO: detect from config

    let exit_reason = tokio::select! {
        res = &mut h_indexer => {
            info!("Indexer completed successfully");
            res?;
            ExitReason::Completed
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down...");
            ExitReason::UserInterrupt
        }
        _ = wait_for_sigterm() => {
            info!("Received SIGTERM, shutting down...");
            ExitReason::Terminated
        }
    };

    cancel.cancel();
    info!("Waiting for graceful shutdown...");
    let _ = h_indexer.await;

    match exit_reason {
        ExitReason::Completed => Ok(()),
        ExitReason::UserInterrupt => Ok(()),
        ExitReason::Terminated if is_bounded_job => {
            std::process::exit(1);
        }
        ExitReason::Terminated => Ok(()),
    }
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("Failed to install SIGTERM handler")
        .recv()
        .await;
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await
}
