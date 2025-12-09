// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Accumulator for total metrics across all transactions in a replay run.
#[derive(Debug, Default, Clone)]
pub struct TotalMetrics {
    pub tx_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub total_ms: u128,
    pub exec_ms: u128,
}

impl TotalMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accumulate metrics from a single transaction replay.
    pub fn add_transaction(&mut self, success: bool, total_ms: u128, exec_ms: u128) {
        self.tx_count += 1;
        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.total_ms += total_ms;
        self.exec_ms += exec_ms;
    }
}
