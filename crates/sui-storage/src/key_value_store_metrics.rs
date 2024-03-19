// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry, HistogramVec,
    IntCounterVec, Registry,
};
use std::sync::Arc;

pub struct KeyValueStoreMetrics {
    pub key_value_store_num_fetches_success: IntCounterVec,
    pub key_value_store_num_fetches_not_found: IntCounterVec,
    pub key_value_store_num_fetches_error: IntCounterVec,

    pub key_value_store_num_fetches_latency_ms: HistogramVec,
    pub key_value_store_num_fetches_batch_size: HistogramVec,
}

impl KeyValueStoreMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            key_value_store_num_fetches_success: register_int_counter_vec_with_registry!(
                "key_value_store_num_fetches_success",
                "Number of successful fetches from key value store",
                &["store", "type"],
                registry,
            )
            .unwrap(),
            key_value_store_num_fetches_not_found: register_int_counter_vec_with_registry!(
                "key_value_store_num_fetches_not_found",
                "Number of fetches from key value store that returned not found",
                &["store", "type"],
                registry,
            )
            .unwrap(),
            key_value_store_num_fetches_error: register_int_counter_vec_with_registry!(
                "key_value_store_num_fetches_error",
                "Number of fetches from key value store that returned an error",
                &["store", "type"],
                registry,
            )
            .unwrap(),

            key_value_store_num_fetches_latency_ms: register_histogram_vec_with_registry!(
                "key_value_store_num_fetches_latency_ms",
                "Latency of fetches from key value store",
                &["store", "type"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),

            key_value_store_num_fetches_batch_size: register_histogram_vec_with_registry!(
                "key_value_store_num_fetches_batch_size",
                "Number of keys fetched per batch",
                &["store", "type"],
                prometheus::exponential_buckets(1.0, 1.6, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
        })
    }

    pub fn new_for_tests() -> Arc<Self> {
        Self::new(&Registry::new())
    }
}
