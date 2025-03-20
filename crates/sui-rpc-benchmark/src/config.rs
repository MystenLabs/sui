// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of concurrent clients
    pub concurrency: usize,
    /// Duration to run the benchmark in seconds
    /// If None, the benchmark will run until all requests are processed
    pub duration: Option<Duration>,
    /// Optional path to a jsonl file for JSON RPC benchmark.
    /// The file contains a list of JSON RPC requests that are collected from Grafana,
    /// and will be run concurrently by the JSON RPC benchmark runner.
    pub json_rpc_file_path: Option<String>,
    /// List of methods to skip during benchmark.
    /// These methods will not be sent to the JSON RPC server.
    pub json_rpc_methods_to_skip: HashSet<String>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            concurrency: 50,
            duration: Some(Duration::from_secs(30)),
            json_rpc_file_path: None,
            json_rpc_methods_to_skip: HashSet::new(),
        }
    }
}
