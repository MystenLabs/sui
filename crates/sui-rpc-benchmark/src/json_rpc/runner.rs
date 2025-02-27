// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::request_loader::JsonRpcRequestLine;
use crate::config::BenchmarkConfig;
/// This module implements the JSON RPC benchmark runner.
/// The main function is `run_queries`, which runs the queries concurrently
/// and records the overall and per-method stats.
use anyhow::{Context as _, Result};
use dashmap::DashMap;
use phf::phf_map;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use sui_indexer_alt_framework::task::TrySpawnStreamExt;
use tokio::time::timeout;
use tracing::{debug, info};

/// static map of method names to the index of their cursor parameter
static METHOD_CURSOR_POSITIONS: phf::Map<&'static str, usize> = phf_map! {
    // based on function headers in crates/sui-json-rpc-api/src/indexer.rs
    "suix_getOwnedObjects" => 2,
    "suix_queryTransactionBlocks" => 1,
    // based on function headers in crates/sui-json-rpc-api/src/coin.rs
    "suix_getCoins" => 2,
    "suix_getAllCoins" => 1,
};

/// Statistics for a single JSON RPC method
#[derive(Clone, Default)]
pub struct PerMethodStats {
    pub total_sent: usize,
    pub total_errors: usize,
    pub total_latency_ms: f64,
}

/// Aggregated statistics for all JSON RPC requests
#[derive(Clone, Default)]
pub struct JsonRpcStats {
    pub total_sent: usize,
    pub total_errors: usize,
    pub total_latency_ms: f64,
    pub per_method: HashMap<String, PerMethodStats>,
}

/// Tracks pagination state for active pagination requests
/// The key is a tuple of method name and the params `Vec<Value>`, where the cursor parameter is set to `null`.
/// The value is the cursor for the next page.
#[derive(Default)]
struct PaginationCursorState {
    requests: DashMap<(String, Vec<Value>), Value>,
}

impl JsonRpcStats {
    pub fn new() -> Self {
        Self::default()
    }

    fn record_request(&mut self, method: &str, latency_ms: f64, is_error: bool) {
        self.total_sent += 1;
        self.total_latency_ms += latency_ms;
        if is_error {
            self.total_errors += 1;
        }

        let method_stats = self.per_method.entry(method.to_string()).or_default();
        method_stats.total_sent += 1;
        method_stats.total_latency_ms += latency_ms;
        if is_error {
            method_stats.total_errors += 1;
        }
    }
}

impl PaginationCursorState {
    fn new() -> Self {
        Self {
            requests: DashMap::new(),
        }
    }

    /// Returns the index of the cursor parameter for a method, if it exists;
    /// Otherwise, it means no cursor transformation is needed for this method.
    fn get_method_cursor_index(method: &str) -> Option<usize> {
        METHOD_CURSOR_POSITIONS.get(method).copied()
    }

    fn get_method_key(
        method: &str,
        params: &[Value],
    ) -> Result<(String, Vec<Value>), anyhow::Error> {
        let cursor_idx = METHOD_CURSOR_POSITIONS
            .get(method)
            .with_context(|| format!("method {} not found in cursor positions", method))?;
        let mut key_params = params.to_vec();
        if let Some(param_to_modify) = key_params.get_mut(*cursor_idx) {
            *param_to_modify = Value::Null;
        } else {
            // cursor parameter is omitted for this method, thus do nothing
            debug!("cursor parameter is omitted for method {}", method);
        }
        Ok((method.to_string(), key_params))
    }

    fn update_params_cursor(
        params: &mut Value,
        cursor_idx: usize,
        new_cursor: Option<&Value>,
        method: &str,
    ) -> Result<(), anyhow::Error> {
        let params_array = params
            .get_mut("params")
            .and_then(|v| v.as_array_mut())
            .with_context(|| format!("params not found or not an array for method {}", method))?;
        if let Some(param_to_modify) = params_array.get_mut(cursor_idx) {
            *param_to_modify = match new_cursor {
                Some(cursor) => cursor.clone(),
                None => Value::Null,
            };
        } else {
            debug!("cursor parameter is omitted for method {}", method);
        }
        Ok(())
    }

    fn update(&self, key: (String, Vec<Value>), cursor: Option<Value>) {
        if let Some(cursor) = cursor {
            self.requests.insert(key, cursor);
        } else {
            self.requests.remove(&key);
        }
    }

    fn get(&self, key: &(String, Vec<Value>)) -> Option<Value> {
        self.requests.get(key).map(|entry| entry.clone())
    }
}

pub async fn run_queries(
    endpoint: &str,
    requests: &[JsonRpcRequestLine],
    config: &BenchmarkConfig,
) -> Result<JsonRpcStats> {
    let concurrency = config.concurrency;
    let shared_stats = Arc::new(Mutex::new(JsonRpcStats::new()));
    let pagination_state = Arc::new(PaginationCursorState::new());
    let client = reqwest::Client::new();
    let endpoint = endpoint.to_owned();

    info!("Skipping methods: {:?}", config.json_rpc_methods_to_skip);
    let requests: Vec<_> = requests
        .iter()
        .filter(|r| !config.json_rpc_methods_to_skip.contains(&r.method))
        .cloned()
        .collect();
    let stats = shared_stats.clone();

    let tasks = futures::stream::iter(requests.into_iter()).try_for_each_spawned(
        concurrency,
        |mut request_line| {
            let task_stats = stats.clone();
            let client = client.clone();
            let endpoint = endpoint.clone();
            let pagination_state = pagination_state.clone();

            // adapt pagination cursor to new cursor format if needed
            async move {
                let params = request_line
                    .body_json
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| a.to_vec())
                    .with_context(|| {
                        format!(
                            "params not found or not an array for method: {}",
                            request_line.method
                        )
                    })?;

                if let Some(cursor_idx) =
                    PaginationCursorState::get_method_cursor_index(&request_line.method)
                {
                    let method_key =
                        PaginationCursorState::get_method_key(&request_line.method, &params)?;
                    PaginationCursorState::update_params_cursor(
                        &mut request_line.body_json,
                        cursor_idx,
                        pagination_state.get(&method_key).as_ref(),
                        &request_line.method,
                    )?;
                }

                let now = Instant::now();
                let res = timeout(
                    Duration::from_secs(10),
                    client.post(&endpoint).json(&request_line.body_json).send(),
                )
                .await;
                let elapsed_ms = now.elapsed().as_millis() as f64;

                // update pagination cursor if the request is successful.
                let mut is_error = true;
                if let Ok(Ok(resp)) = res {
                    if resp.status().is_success() {
                        #[derive(Deserialize)]
                        struct Body {
                            result: Result,
                        }
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Result {
                            has_next_page: bool,
                            next_cursor: Option<Value>,
                        }

                        if let Ok(Body { result }) = resp.json().await {
                            let method_key = PaginationCursorState::get_method_key(
                                &request_line.method,
                                &params,
                            )?;
                            pagination_state.update(
                                method_key,
                                if result.has_next_page {
                                    result.next_cursor
                                } else {
                                    None
                                },
                            );
                            is_error = false;
                        }
                    }
                }

                // Record stats after all async operations to avoid error of sending future between threads
                let mut stats = task_stats
                    .lock()
                    .expect("Thread holding stats lock panicked");
                stats.record_request(&request_line.method, elapsed_ms, is_error);
                Ok::<(), anyhow::Error>(())
            }
        },
    );

    timeout(config.duration, tasks).await.unwrap_or(Ok(()))?;
    let final_stats = shared_stats
        .lock()
        .expect("Thread holding stats lock panicked")
        .clone();
    Ok(final_stats)
}
