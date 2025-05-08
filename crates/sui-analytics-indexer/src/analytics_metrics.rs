// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec,
    Registry,
};

#[derive(Clone)]
pub struct AnalyticsMetrics {
    pub total_received: IntCounterVec,
    pub last_uploaded_checkpoint: IntGaugeVec,
    pub max_checkpoint_on_store: IntGaugeVec,
    pub total_too_large_to_deserialize: IntCounterVec,
    pub file_size_bytes: HistogramVec,
}

impl AnalyticsMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_received: register_int_counter_vec_with_registry!(
                "total_received",
                "Number of checkpoints received",
                &["data_type"],
                registry
            )
            .unwrap(),
            last_uploaded_checkpoint: register_int_gauge_vec_with_registry!(
                "last_uploaded_checkpoint",
                "Number of uploaded checkpoints.",
                &["data_type"],
                registry,
            )
            .unwrap(),
            max_checkpoint_on_store: register_int_gauge_vec_with_registry!(
                "max_checkpoint_on_store",
                "Max checkpoint on the db table.",
                &["data_type"],
                registry,
            )
            .unwrap(),
            total_too_large_to_deserialize: register_int_counter_vec_with_registry!(
                "total_too_large_to_deserialize",
                "Total number of rows skipped due to size.",
                &["data_type"],
                registry,
            )
            .unwrap(),
            file_size_bytes: register_histogram_vec_with_registry!(
                HistogramOpts::new("file_size_bytes", "Size of generated files in bytes.",)
                    .buckets(vec![
                        1_000.0,         // 1 KB
                        10_000.0,        // 10 KB
                        100_000.0,       // 100 KB
                        1_000_000.0,     // 1 MB
                        10_000_000.0,    // 10 MB
                        50_000_000.0,    // 50 MB
                        100_000_000.0,   // 100 MB
                        250_000_000.0,   // 250 MB
                        500_000_000.0,   // 500 MB
                        1_000_000_000.0, // 1 GB
                        2_000_000_000.0, // 2 GB
                    ]),
                &["source"],
                registry,
            )
            .unwrap(),
        }
    }
}
