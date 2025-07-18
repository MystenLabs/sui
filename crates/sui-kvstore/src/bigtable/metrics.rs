// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry, HistogramVec,
    IntCounterVec, Registry,
};
use std::sync::Arc;

pub(crate) struct KvMetrics {
    pub kv_get_success: IntCounterVec,
    pub kv_get_not_found: IntCounterVec,
    pub kv_get_errors: IntCounterVec,
    pub kv_get_latency_ms: HistogramVec,
    pub kv_get_batch_size: HistogramVec,
    pub kv_get_latency_ms_per_key: HistogramVec,
    pub kv_scan_success: IntCounterVec,
    pub kv_scan_not_found: IntCounterVec,
    pub kv_scan_error: IntCounterVec,
    pub kv_scan_latency_ms: HistogramVec,
    pub kv_bt_chunk_latency_ms: HistogramVec,
    pub kv_bt_chunk_rows_returned_count: IntCounterVec,
    pub kv_bt_chunk_rows_seen_count: IntCounterVec,
}

impl KvMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            kv_get_success: register_int_counter_vec_with_registry!(
                "kv_get_success",
                "Number of successful fetches from kv store",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_not_found: register_int_counter_vec_with_registry!(
                "kv_get_not_found",
                "Number of fetches from kv store that returned not found",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_errors: register_int_counter_vec_with_registry!(
                "kv_get_errors",
                "Number of fetches from kv store that returned an error",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_latency_ms: register_histogram_vec_with_registry!(
                "kv_get_latency_ms",
                "Latency of fetches from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_batch_size: register_histogram_vec_with_registry!(
                "kv_get_batch_size",
                "Number of keys fetched per batch from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_latency_ms_per_key: register_histogram_vec_with_registry!(
                "kv_get_latency_ms_per_key",
                "Latency of fetches from kv store per key",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_scan_success: register_int_counter_vec_with_registry!(
                "kv_scan_success",
                "Number of successful scans from kv store",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_not_found: register_int_counter_vec_with_registry!(
                "kv_scan_not_found",
                "Number of fetches from kv store that returned not found",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_error: register_int_counter_vec_with_registry!(
                "kv_scan_error",
                "Number of scans from kv store that returned an error",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_latency_ms: register_histogram_vec_with_registry!(
                "kv_scan_latency_ms",
                "Latency of scans from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_bt_chunk_latency_ms: register_histogram_vec_with_registry!(
                "kv_bt_chunk_latency_ms",
                "Reported BigTable latency for a single chunk",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_bt_chunk_rows_returned_count: register_int_counter_vec_with_registry!(
                "kv_bt_chunk_rows_returned_count",
                "Reported BigTable rows returned count for a single chunk",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_bt_chunk_rows_seen_count: register_int_counter_vec_with_registry!(
                "kv_bt_chunk_rows_seen_count",
                "Reported BigTable rows seen count for a single chunk",
                &["client", "table"],
                registry,
            )
            .unwrap(),
        })
    }
}
