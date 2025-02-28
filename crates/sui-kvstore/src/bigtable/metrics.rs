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
        })
    }
}
