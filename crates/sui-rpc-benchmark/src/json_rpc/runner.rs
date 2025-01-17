// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements the JSON RPC benchmark runner.
/// The main function is `run_queries`, which runs the queries concurrently
/// and records the metrics.
use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use sui_indexer_alt_framework::task::TrySpawnStreamExt;
use tokio::time::timeout;

use super::request_loader::JsonRpcRequestLine;
use crate::config::BenchmarkConfig;

#[derive(Clone, Default)]
pub struct MethodMetrics {
    pub total_sent: usize,
    pub total_errors: usize,
    // record total latency and calculate average latency later to avoid duplicate calculations
    pub total_latency_ms: f64,
}

#[derive(Clone, Default)]
pub struct JsonRpcMetrics {
    pub total_sent: usize,
    pub total_errors: usize,
    // record total latency and calculate average latency to avoid duplicate calculations
    pub total_latency_ms: f64,
    pub per_method: HashMap<String, MethodMetrics>,
}

impl JsonRpcMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    fn record_request(&mut self, method: &str, latency_ms: f64, is_error: bool) {
        self.total_sent += 1;
        self.total_latency_ms += latency_ms;
        if is_error {
            self.total_errors += 1;
        }

        let method_metrics = self.per_method.entry(method.to_string()).or_default();
        method_metrics.total_sent += 1;
        method_metrics.total_latency_ms += latency_ms;
        if is_error {
            method_metrics.total_errors += 1;
        }
    }
}

pub async fn run_queries(
    endpoint: &str,
    requests: &[JsonRpcRequestLine],
    config: &BenchmarkConfig,
) -> Result<JsonRpcMetrics> {
    let concurrency = config.concurrency;
    let shared_metrics = Arc::new(Mutex::new(JsonRpcMetrics::new()));
    let client = reqwest::Client::new();
    let endpoint = endpoint.to_owned();
    let requests = requests.to_vec();
    let metrics = shared_metrics.clone();
    futures::stream::iter(requests.into_iter().map(move |request_line| {
        let task_metrics = metrics.clone();
        let client = client.clone();
        let endpoint = endpoint.clone();
        async move {
            let now = Instant::now();
            let res = timeout(
                std::time::Duration::from_secs(10),
                client.post(&endpoint).json(&request_line.body_json).send(),
            )
            .await;

            let elapsed_ms = now.elapsed().as_millis() as f64;
            let is_error = !matches!(res, Ok(Ok(ref resp)) if resp.status().is_success());

            let mut metrics = task_metrics
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire metrics lock: {}", e))?;
            metrics.record_request(&request_line.method, elapsed_ms, is_error);
            Ok::<(), anyhow::Error>(())
        }
    }))
    .try_for_each_spawned(concurrency, |fut| fut)
    .await?;

    let final_metrics = shared_metrics
        .lock()
        .map_err(|e| anyhow::anyhow!("Failed to acquire metrics lock for final results: {}", e))?
        .clone();
    Ok(final_metrics)
}
