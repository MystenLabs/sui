// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use prometheus::Registry;
use std::env;
use sui_analytics_indexer::metrics::Metrics;
use sui_analytics_indexer::{IndexerConfig, build_analytics_indexer, spawn_snowflake_monitors};
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

    let cancel = tokio_util::sync::CancellationToken::new();

    let is_bounded_job = config.last_checkpoint.is_some();

    // Create metrics for Snowflake monitoring
    let metrics = Metrics::new(&registry);

    // Spawn Snowflake monitor tasks (if configured)
    let sf_handles = spawn_snowflake_monitors(&config, metrics.clone(), cancel.clone())?;

    let indexer = build_analytics_indexer(config, metrics, registry, cancel.clone()).await?;
    let mut h_indexer = indexer.run().await?;

    enum ExitReason {
        Completed,
        UserInterrupt,
        Terminated,
    }

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
    for handle in sf_handles {
        let _ = handle.await;
    }

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
