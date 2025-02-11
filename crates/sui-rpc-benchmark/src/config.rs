// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of concurrent clients
    pub concurrency: usize,
    /// Duration to run the benchmark in seconds
    pub duration: Duration,
    /// Optional path to a jsonl file for JSON RPC benchmarks.
    /// The file contains a list of JSON RPC requests that are collected from Grafana,
    /// and will be run concurrently by the JSON RPC benchmark runner.
    pub json_rpc_file_path: Option<String>,
    /// Optional duration for JSON RPC benchmark pagination window.
    /// This is for handling conversion to new pagination cursors on `alt` stack.
    pub json_rpc_pagination_window: Option<Duration>,
    /// List of methods to skip during benchmark.
    /// These methods will not be sent to the JSON RPC server.
    pub json_rpc_methods_to_skip: HashSet<String>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        let json_rpc_methods_to_skip = HashSet::from_iter(["suix_getAllCoins".to_string()]);
        Self {
            concurrency: 50,
            duration: Duration::from_secs(30),
            json_rpc_file_path: None,
            json_rpc_pagination_window: None,
            json_rpc_methods_to_skip,
        }
    }
}
