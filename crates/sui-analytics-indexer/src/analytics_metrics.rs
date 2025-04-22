// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};

#[derive(Clone)]
pub struct AnalyticsMetrics {
    pub total_received: IntCounterVec,
    pub last_uploaded_checkpoint: IntGaugeVec,
    pub max_checkpoint_on_store: IntGaugeVec,
    pub total_too_large_to_deserialize: IntCounterVec,
    pub file_size: IntGaugeVec,
    pub package_fetch_latency: HistogramVec,
    pub package_cache_gets: IntCounterVec,
    pub package_cache_hits: IntCounterVec,
    pub checkpoint_processing_time: HistogramVec,
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
            file_size: register_int_gauge_vec_with_registry!(
                "file_size_bytes",
                "Size of generated files in bytes.",
                &["data_type"],
                registry,
            )
            .unwrap(),
            package_fetch_latency: register_histogram_vec_with_registry!(
                "package_fetch_latency_seconds",
                "Latency of HTTP calls to fetch package data in seconds.",
                &["source"],
                registry,
            )
            .unwrap(),
            package_cache_gets: register_int_counter_vec_with_registry!(
                "package_cache_gets_total",
                "Total number of package cache get requests",
                &[],
                registry
            )
            .unwrap(),
            package_cache_hits: register_int_counter_vec_with_registry!(
                "package_cache_hits_total",
                "Total number of package cache hits",
                &[],
                registry
            )
            .unwrap(),
            checkpoint_processing_time: register_histogram_vec_with_registry!(
                "checkpoint_processing_time_seconds",
                "Time taken to process a checkpoint in seconds",
                &["data_type"],
                registry
            )
            .unwrap(),
        }
    }
}
