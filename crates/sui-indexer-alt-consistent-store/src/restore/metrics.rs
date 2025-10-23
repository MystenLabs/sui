// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, Registry,
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};

/// Histogram buckets for the distribution of object file fetching latencies.
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

pub(super) struct RestorerMetrics {
    pub(crate) objects_fetch_latency: Histogram,
    pub(crate) total_objects_fetch_retries: IntCounter,
    pub(crate) total_bytes_fetched: IntCounter,
    pub(crate) total_partitions_fetched: IntCounter,
    pub(crate) total_partitions_skipped: IntCounter,
    pub(crate) total_partitions_broadcast: IntCounter,
    pub(crate) total_partitions: IntGauge,
    pub(crate) total_partitions_received: IntCounterVec,
    pub(crate) total_objects_received: IntCounterVec,
    pub(crate) total_partitions_restored: IntCounterVec,
    pub(crate) total_objects_restored: IntCounterVec,
    pub(crate) worker_partition_latency: HistogramVec,
}

impl RestorerMetrics {
    pub(super) fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("restorer");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            objects_fetch_latency: register_histogram_with_registry!(
                name("objects_fetch_latency"),
                "Time taken to fetch an object file from a formal snapshot, in seconds",
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
                "Total number of bytes fetched from a formal snapshot",
                registry,
            )
            .unwrap(),

            total_partitions_fetched: register_int_counter_with_registry!(
                name("total_partitions_fetched"),
                "Total number of object file partitions fetched from a formal snapshot",
                registry,
            )
            .unwrap(),

            total_partitions_skipped: register_int_counter_with_registry!(
                name("total_partitions_skipped"),
                "Total number of object file partitions skipped (because all pipelines had already restored them)",
                registry,
            )
            .unwrap(),

            total_partitions_broadcast: register_int_counter_with_registry!(
                name("total_partitions_broadcast"),
                "Total number of object file partitions broadcast to all pipelines",
                registry,
            )
            .unwrap(),

            total_partitions: register_int_gauge_with_registry!(
                name("total_partitions"),
                "The number of object file partitions to be fetched from a formal snapshot",
                registry,
            )
            .unwrap(),

            total_partitions_received: register_int_counter_vec_with_registry!(
                name("total_partitions_received"),
                "Total number of object file partitions received, per pipeline",
                &["pipeline"],
                registry,
            )
            .unwrap(),

            total_objects_received: register_int_counter_vec_with_registry!(
                name("total_objects_received"),
                "Total number of objects received, per pipeline",
                &["pipeline"],
                registry,
            )
            .unwrap(),

            total_partitions_restored: register_int_counter_vec_with_registry!(
                name("total_partitions_restored"),
                "Total number of partitions restored, per pipeline",
                &["pipeline"],
                registry,
            )
            .unwrap(),

            total_objects_restored: register_int_counter_vec_with_registry!(
                name("total_objects_restored"),
                "Total number of objects restored, per pipeline",
                &["pipeline"],
                registry,
            )
            .unwrap(),

            worker_partition_latency: register_histogram_vec_with_registry!(
                name("worker_partition_latency"),
                "Time taken by a worker to process a partition, in seconds",
                &["pipeline"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        })
    }
}
