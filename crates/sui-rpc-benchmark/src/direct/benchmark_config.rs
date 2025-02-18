// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub struct BenchmarkConfig {
    /// Number of concurrent clients
    pub concurrency: usize,
    /// All queries will execute if they finish within the specified timeout.
    /// Otherwise, the binary will stop at that time and report collected metrics.
    pub timeout: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            concurrency: 50,
            timeout: Duration::from_secs(30),
        }
    }
}
