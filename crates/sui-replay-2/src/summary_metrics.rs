// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};

/// Per-transaction metrics structure (shared atomic state)
pub(crate) struct TxMetrics {
    query_txn: (AtomicU64, AtomicU64),   // (ms, count)
    query_objs: (AtomicU64, AtomicU64),  // (ms, count)
    query_epoch: (AtomicU64, AtomicU64), // (ms, count)
}

impl TxMetrics {
    const fn new() -> Self {
        Self {
            query_txn: (AtomicU64::new(0), AtomicU64::new(0)),
            query_objs: (AtomicU64::new(0), AtomicU64::new(0)),
            query_epoch: (AtomicU64::new(0), AtomicU64::new(0)),
        }
    }

    fn reset(&self) {
        self.query_txn.0.store(0, Ordering::Relaxed);
        self.query_txn.1.store(0, Ordering::Relaxed);
        self.query_objs.0.store(0, Ordering::Relaxed);
        self.query_objs.1.store(0, Ordering::Relaxed);
        self.query_epoch.0.store(0, Ordering::Relaxed);
        self.query_epoch.1.store(0, Ordering::Relaxed);
    }

    fn add_txn(&self, ms: u128) {
        self.query_txn.0.fetch_add(ms as u64, Ordering::Relaxed);
        self.query_txn.1.fetch_add(1, Ordering::Relaxed);
    }

    fn add_objs(&self, ms: u128) {
        self.query_objs.0.fetch_add(ms as u64, Ordering::Relaxed);
        self.query_objs.1.fetch_add(1, Ordering::Relaxed);
    }

    fn add_epoch(&self, ms: u128) {
        self.query_epoch.0.fetch_add(ms as u64, Ordering::Relaxed);
        self.query_epoch.1.fetch_add(1, Ordering::Relaxed);
    }
}

static TX_METRICS: TxMetrics = TxMetrics::new();

// Reset per-transaction metrics (timers and query counts).
pub(crate) fn tx_metrics_reset() {
    TX_METRICS.reset();
}

// Add elapsed milliseconds to the transaction-data query timer and increment count.
pub(crate) fn tx_metrics_add_txn(ms: u128) {
    TX_METRICS.add_txn(ms);
}

// Add elapsed milliseconds to the objects query timer and increment count.
pub(crate) fn tx_metrics_add_objs(ms: u128) {
    TX_METRICS.add_objs(ms);
}

// Add elapsed milliseconds to the epoch query timer and increment count.
pub(crate) fn tx_metrics_add_epoch(ms: u128) {
    TX_METRICS.add_epoch(ms);
}

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
