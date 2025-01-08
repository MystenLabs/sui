// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub struct BenchmarkConfig {
    // Number of concurrent clients
    pub concurrency: usize,
    // Duration to run the benchmark in seconds
    pub duration: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            concurrency: 50,
            duration: Duration::from_secs(30),
        }
    }
}
