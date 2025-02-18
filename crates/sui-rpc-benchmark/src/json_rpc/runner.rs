// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::request_loader::JsonRpcRequestLine;
use crate::config::BenchmarkConfig;
/// This module implements the JSON RPC benchmark runner.
/// The main function is `run_queries`, which runs the queries concurrently
/// and records the overall and per-method stats.
use anyhow::{bail, Result};
use dashmap::DashMap;
use phf::phf_map;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use sui_indexer_alt_framework::task::TrySpawnStreamExt;
use tokio::time::timeout;
use tracing::info;

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
/// The key is the method name and the params serialized to a string, except the cursor parameter;
/// The value is the cursor for the next page.
#[derive(Default)]
struct PaginationCursorState {
    requests: DashMap<String, Value>,
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

    fn get_method_key(method: &str, params: &[Value]) -> Result<String, anyhow::Error> {
        let cursor_idx = METHOD_CURSOR_POSITIONS
            .get(method)
            .ok_or_else(|| anyhow::anyhow!("method {} not found in cursor positions", method))?;
        let key_param_len = params.len();
        let mut key_params = params.to_vec();
        let param_to_modify = key_params.get_mut(*cursor_idx).ok_or_else(|| {
            anyhow::anyhow!(
                "params length {} is less than cursor index {} for method {}",
                key_param_len,
                cursor_idx,
                method
            )
        })?;
        *param_to_modify = Value::Null;
        serde_json::to_string(&key_params)
            .map(|params_str| format!("{}-{}", method, params_str))
            .map_err(|e| anyhow::anyhow!("failed to generate key for method {}: {}", method, e))
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
            .ok_or_else(|| {
                anyhow::anyhow!("params not found or not an array for method {}", method)
            })?;
        if params_array.len() <= cursor_idx {
            return Err(anyhow::anyhow!(
                "cursor index {} is out of bounds for method {}",
                cursor_idx,
                method
            ));
        }
        params_array[cursor_idx] = match new_cursor {
            Some(cursor) => cursor.clone(),
            None => Value::Null,
        };
        Ok(())
    }

    fn update(&self, key: String, cursor: Option<Value>) {
        if let Some(cursor) = cursor {
            self.requests.insert(key, cursor);
        } else {
            self.requests.remove(&key);
        }
    }

    fn get(&self, key: &str) -> Option<Value> {
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

    let stream = futures::stream::iter(requests.into_iter().map(move |mut request_line| {
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
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "params not found or not an array for method: {}",
                        request_line.method
                    )
                })?;

            if let Some(cursor_idx) =
                PaginationCursorState::get_method_cursor_index(&request_line.method)
            {
                let method_key =
                    PaginationCursorState::get_method_key(&request_line.method, &params)?;

                if let Some(param_value) = params.get(cursor_idx) {
                    // only update cursor if the original value is not Null, otherwise stick to the Null value,
                    // which means JSON RPC server will return the first page of results.
                    if *param_value != Value::Null {
                        if let Some(cursor) = pagination_state.get(&method_key) {
                            // use stored cursor if available, continue pagination
                            PaginationCursorState::update_params_cursor(
                                &mut request_line.body_json,
                                cursor_idx,
                                Some(&cursor),
                                &request_line.method,
                            )?;
                        } else {
                            // otherwise, clear cursor to reset the pagination
                            PaginationCursorState::update_params_cursor(
                                &mut request_line.body_json,
                                cursor_idx,
                                None,
                                &request_line.method,
                            )?;
                        }
                    }
                } else {
                    bail!(
                        "Could not read cursor index {} for method {} with params {:?}",
                        cursor_idx,
                        request_line.method,
                        params
                    );
                }
            }

            let now = Instant::now();
            let res = timeout(
                Duration::from_secs(10),
                client.post(&endpoint).json(&request_line.body_json).send(),
            )
            .await;
            let elapsed_ms = now.elapsed().as_millis() as f64;
            let is_error = !matches!(res, Ok(Ok(ref resp)) if resp.status().is_success());

            // update pagination cursor if the request is successful.
            if !is_error {
                let method_key =
                    PaginationCursorState::get_method_key(&request_line.method, &params)?;
                if let Ok(Ok(resp)) = res {
                    let body = resp.json::<Value>().await?;
                    // check if there is a next page, if so, update the cursor with the nextCursor;
                    // otherwise, remove the cursor so that next request will restart from the first page.
                    let has_next_page = body
                        .get("result")
                        .ok_or_else(|| {
                            anyhow::anyhow!("no result found for method {}", request_line.method)
                        })?
                        .get("hasNextPage")
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "no hasNextPage found for method {}",
                                request_line.method
                            )
                        })?
                        .as_bool()
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "hasNextPage is not a boolean for method {}",
                                request_line.method
                            )
                        })?;
                    if has_next_page {
                        let cursor = body
                            .get("result")
                            .and_then(|r| r.get("nextCursor"))
                            .cloned();
                        pagination_state.update(method_key, cursor);
                    } else {
                        pagination_state.update(method_key, None);
                    }
                }
            }

            // Record stats after all async operations to avoid error of sending future between threads
            let mut stats = task_stats
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire stats lock: {}", e))?;
            stats.record_request(&request_line.method, elapsed_ms, is_error);
            drop(stats); // Explicitly drop the MutexGuard

            Ok::<(), anyhow::Error>(())
        }
    }));

    timeout(
        config.duration,
        stream.try_for_each_spawned(concurrency, |fut| fut),
    )
    .await
    .unwrap_or(Ok(()))?;

    let final_stats = shared_stats
        .lock()
        .map_err(|e| anyhow::anyhow!("Failed to acquire stats lock for final results: {}", e))?
        .clone();
    Ok(final_stats)
}
