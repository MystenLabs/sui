// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of concurrent clients
    pub concurrency: usize,
    /// Duration to run the benchmark in seconds
    pub duration: Duration,
    /// Optional path to JSON RPC file for JSON RPC benchmarks
    pub json_rpc_file_path: Option<String>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            concurrency: 50,
            duration: Duration::from_secs(30),
            json_rpc_file_path: None,
        }
    }
}
