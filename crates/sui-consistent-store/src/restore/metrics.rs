// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Prometheus metrics for the restore path.
//!
//! [`FormalSnapshotMetrics`] covers the formal-snapshot
//! [`RestoreSource`](super::RestoreSource): "partitions" are
//! per-bucket files and "objects" are decoded live objects. It is
//! adapted from `sui-indexer-alt-consistent-store::restore::metrics`;
//! per-pipeline counters are dropped because the
//! [`RestoreDriver`](super::RestoreDriver) — not the source — knows
//! which pipelines see each object now.
//!
//! [`RestoreMetrics`] covers the source-agnostic
//! [`RestoreDriver`](super::RestoreDriver). Its per-shard completion
//! counter is derived from the persisted `__restore` cursors, so —
//! unlike the source's per-session fetch counters — it reflects
//! cumulative progress that survives a restart and resume.

use std::sync::Arc;

use prometheus::Histogram;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_histogram_with_registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;

/// Buckets for fetch latency, in seconds.
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

pub struct FormalSnapshotMetrics {
    pub(super) objects_fetch_latency: Histogram,
    pub(super) total_objects_fetch_retries: IntCounter,
    pub(super) total_bytes_fetched: IntCounter,
    pub(super) total_partitions_fetched: IntCounter,
    pub(super) total_partitions: IntGauge,
}

impl FormalSnapshotMetrics {
    pub fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("formal_snapshot");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            objects_fetch_latency: register_histogram_with_registry!(
                name("objects_fetch_latency"),
                "Time taken to fetch one `.obj` file from a formal snapshot, in seconds",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            total_objects_fetch_retries: register_int_counter_with_registry!(
                name("total_objects_fetch_retries"),
                "Total number of retries when fetching object files from a formal snapshot",
                registry,
            )
            .unwrap(),

            total_bytes_fetched: register_int_counter_with_registry!(
                name("total_bytes_fetched"),
                "Total bytes fetched from a formal snapshot (decompressed length excluded)",
                registry,
            )
            .unwrap(),

            total_partitions_fetched: register_int_counter_with_registry!(
                name("total_partitions_fetched"),
                "Total object-file partitions successfully fetched from a formal snapshot",
                registry,
            )
            .unwrap(),

            total_partitions: register_int_gauge_with_registry!(
                name("total_partitions"),
                "The total number of object-file partitions discovered in the snapshot",
                registry,
            )
            .unwrap(),
        })
    }
}

/// Source-agnostic metrics for the [`RestoreDriver`](super::RestoreDriver).
///
/// Shard completion is derived from the persisted `__restore`
/// cursors, so `restore_shards_done` reflects cumulative progress
/// across a crash and resume — distinct from
/// [`FormalSnapshotMetrics`]'s `total_partitions_fetched`, which
/// counts only the current process's fetches.
pub struct RestoreMetrics {
    pub(super) restore_shards_total: IntGauge,
    pub(super) restore_shards_done: IntGauge,
}

impl RestoreMetrics {
    pub fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("restore");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            restore_shards_total: register_int_gauge_with_registry!(
                name("restore_shards_total"),
                "Total number of shards the restore must drive to completion",
                registry,
            )
            .unwrap(),

            restore_shards_done: register_int_gauge_with_registry!(
                name("restore_shards_done"),
                "Number of shards fully restored, counting those already complete on \
                 resume; reaches restore_shards_total when the restore finishes",
                registry,
            )
            .unwrap(),
        })
    }
}
