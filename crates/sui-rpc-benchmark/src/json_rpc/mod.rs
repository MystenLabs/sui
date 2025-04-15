// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::BenchmarkConfig;
use anyhow::Result;
use request_loader::load_json_rpc_requests;
use runner::run_queries;
use std::{collections::HashSet, time::Duration};
use tracing::info;

pub mod request_loader;
pub mod runner;

pub async fn run_benchmark(
    endpoint: &str,
    file_path: &str,
    concurrency: usize,
    duration_secs: Option<u64>,
    json_rpc_methods_to_skip: HashSet<String>,
) -> Result<()> {
    let config = BenchmarkConfig {
        concurrency,
        duration: duration_secs.map(Duration::from_secs),
        json_rpc_file_path: Some(file_path.to_string()),
        json_rpc_methods_to_skip,
    };

    info!("Loading JSON RPC requests from {}", file_path);
    let requests = load_json_rpc_requests(file_path)?;
    info!("Loaded {} requests", requests.len());

    let metrics = run_queries(endpoint, &requests, &config).await?;
    info!("Benchmark results:");
    info!("=== Overall Statistics ===");
    info!("Total requests sent: {}", metrics.total_sent);
    info!("Total errors: {}", metrics.total_errors);
    if metrics.total_sent > 0 {
        let avg_latency = metrics.total_latency_ms / metrics.total_sent as f64;
        info!("Average latency: {:.2}ms", avg_latency);
        let success_rate = ((metrics.total_sent - metrics.total_errors) as f64
            / metrics.total_sent as f64)
            * 100.0;
        info!("Success rate: {:.1}%", success_rate);
    }
    info!("=== Per-Method Statistics ===");
    let mut methods: Vec<_> = metrics.per_method.iter().collect();
    methods.sort_by_key(|(method, _)| *method);
    for (method, stats) in methods {
        info!("Method: {}", method);
        info!("  Requests: {}", stats.total_sent);
        info!("  Errors: {}", stats.total_errors);
        if stats.total_sent > 0 {
            let method_avg_latency = stats.total_latency_ms / stats.total_sent as f64;
            let method_success_rate =
                ((stats.total_sent - stats.total_errors) as f64 / stats.total_sent as f64) * 100.0;
            info!("  Avg latency: {:.2}ms", method_avg_latency);
            info!("  Success rate: {:.1}%", method_success_rate);
        }
    }
    Ok(())
}
