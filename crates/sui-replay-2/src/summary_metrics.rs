// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use tracing::debug;

// Per-transaction query time accumulators (thread-local)
thread_local! {
    static TX_QUERY_METRICS_MS: RefCell<(u128, u128, u128)> = const { RefCell::new((0, 0, 0)) }; // (txn, objs, epoch)
    static TX_OBJS_REQUESTED: RefCell<u64> = const { RefCell::new(0) };
    static TX_QUERY_COUNTS: RefCell<(u64, u64, u64)> = const { RefCell::new((0, 0, 0)) }; // (txn, objs, epoch)
}

// Reset per-transaction metrics (timers, requested-objects counter, and query counts).
pub(crate) fn tx_metrics_reset() {
    TX_QUERY_METRICS_MS.with(|m| *m.borrow_mut() = (0, 0, 0));
    TX_OBJS_REQUESTED.with(|c| *c.borrow_mut() = 0);
    TX_QUERY_COUNTS.with(|c| *c.borrow_mut() = (0, 0, 0));
}

// Add elapsed milliseconds to the transaction-data query timer for this transaction.
pub(crate) fn tx_metrics_add_txn(ms: u128) {
    TX_QUERY_METRICS_MS.with(|m| {
        let (t, o, e) = *m.borrow();
        *m.borrow_mut() = (t + ms, o, e);
    });
}

// Add elapsed milliseconds to the objects query timer for this transaction.
pub(crate) fn tx_metrics_add_objs(ms: u128) {
    TX_QUERY_METRICS_MS.with(|m| {
        let (t, o, e) = *m.borrow();
        *m.borrow_mut() = (t, o + ms, e);
    });
}

// Add elapsed milliseconds to the epoch query timer for this transaction.
pub(crate) fn tx_metrics_add_epoch(ms: u128) {
    TX_QUERY_METRICS_MS.with(|m| {
        let (t, o, e) = *m.borrow();
        *m.borrow_mut() = (t, o, e + ms);
    });
}

// Snapshot of per-transaction query timers: (txn_ms, objs_ms, epoch_ms).
pub(crate) fn tx_metrics_snapshot() -> (u128, u128, u128) {
    TX_QUERY_METRICS_MS.with(|m| *m.borrow())
}

// Increment the number of objects requested in the current multi-get batch.
pub(crate) fn tx_objs_add(n: usize) {
    TX_OBJS_REQUESTED.with(|c| *c.borrow_mut() += n as u64);
}

// Snapshot of how many objects were requested for this transaction.
pub(crate) fn tx_objs_snapshot() -> u64 {
    TX_OBJS_REQUESTED.with(|c| *c.borrow())
}

// Increment the number of transaction-data queries executed for this transaction.
pub(crate) fn tx_counts_add_txn() {
    TX_QUERY_COUNTS.with(|c| {
        let (t, o, e) = *c.borrow();
        *c.borrow_mut() = (t + 1, o, e);
    });
}

// Increment the number of object batches (multi-get objects) executed for this transaction.
pub(crate) fn tx_counts_add_objs() {
    TX_QUERY_COUNTS.with(|c| {
        let (t, o, e) = *c.borrow();
        *c.borrow_mut() = (t, o + 1, e);
    });
}

// Increment the number of epoch-info queries executed for this transaction.
pub(crate) fn tx_counts_add_epoch() {
    TX_QUERY_COUNTS.with(|c| {
        let (t, o, e) = *c.borrow();
        *c.borrow_mut() = (t, o, e + 1);
    });
}

// Snapshot of query counts: (txn_count, objs_count, epoch_count).
pub(crate) fn tx_query_counts_snapshot() -> (u64, u64, u64) {
    TX_QUERY_COUNTS.with(|c| *c.borrow())
}

// Log a concise summary of replay metrics for a transaction at debug level.
pub(crate) fn log_replay_metrics(tx_digest: &str, total_ms: u128, exec_ms: u128) {
    let (txn_ms, objs_ms, epoch_ms) = tx_metrics_snapshot();
    let objs_requested = tx_objs_snapshot();
    let (txn_q, objs_q, epoch_q) = tx_query_counts_snapshot();
    let bucket = if total_ms <= 10_000 {
        "le10"
    } else if total_ms <= 20_000 {
        "gt10le20"
    } else if total_ms <= 30_000 {
        "gt20le30"
    } else if total_ms <= 60_000 {
        "gt30le60"
    } else {
        "gt60"
    };
    debug!(
        "Tx metrics {}: bucket={} total_ms={} exec_ms={} txn_ms={} objs_ms={} epoch_ms={} objs_requested={} q_counts(txn,objs,epoch)=({},{},{})",
        tx_digest,
        bucket,
        total_ms,
        exec_ms,
        txn_ms,
        objs_ms,
        epoch_ms,
        objs_requested,
        txn_q,
        objs_q,
        epoch_q
    );
}
