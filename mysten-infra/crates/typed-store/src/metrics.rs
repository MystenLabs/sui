// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;

use once_cell::sync::OnceCell;
use prometheus::{
    exponential_buckets, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, HistogramVec, IntCounterVec, Registry,
};

#[derive(Debug)]
pub struct DBMetrics {
    pub rocksdb_iter_latency_seconds: HistogramVec,
    pub rocksdb_iter_bytes: HistogramVec,
    pub rocksdb_get_latency_seconds: HistogramVec,
    pub rocksdb_get_bytes: HistogramVec,
    pub rocksdb_multiget_latency_seconds: HistogramVec,
    pub rocksdb_multiget_bytes: HistogramVec,
    pub rocksdb_put_latency_seconds: HistogramVec,
    pub rocksdb_put_bytes: HistogramVec,
    pub rocksdb_delete_latency_seconds: HistogramVec,
    pub rocksdb_deletes: IntCounterVec,
    pub rocksdb_batch_commit_latency_seconds: HistogramVec,
    pub rocksdb_batch_commit_bytes: HistogramVec,
}

impl DBMetrics {
    pub fn make_db_metrics(registry: &Registry) -> &'static Arc<DBMetrics> {
        static ONCE: OnceCell<Arc<DBMetrics>> = OnceCell::new();
        ONCE.get_or_init(|| Arc::new(DBMetrics::new(registry)))
    }
    pub(crate) fn new(registry: &Registry) -> Self {
        DBMetrics {
            rocksdb_iter_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_iter_latency_seconds",
                "Rocksdb iter latency in seconds",
                &["cf_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_iter_bytes: register_histogram_vec_with_registry!(
                "rocksdb_iter_bytes",
                "Rocksdb iter size in bytes",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_get_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_get_latency_seconds",
                "Rocksdb get latency in seconds",
                &["cf_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_get_bytes: register_histogram_vec_with_registry!(
                "rocksdb_get_bytes",
                "Rocksdb get call returned data size in bytes",
                &["cf_name"],
                registry
            )
            .unwrap(),
            rocksdb_multiget_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_multiget_latency_seconds",
                "Rocksdb multiget latency in seconds",
                &["cf_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_multiget_bytes: register_histogram_vec_with_registry!(
                "rocksdb_multiget_bytes",
                "Rocksdb multiget call returned data size in bytes",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_put_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_put_latency_seconds",
                "Rocksdb put latency in seconds",
                &["cf_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_put_bytes: register_histogram_vec_with_registry!(
                "rocksdb_put_bytes",
                "Rocksdb put call puts data size in bytes",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_delete_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_delete_latency_seconds",
                "Rocksdb delete latency in seconds",
                &["cf_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_deletes: register_int_counter_vec_with_registry!(
                "rocksdb_deletes",
                "Rocksdb delete calls",
                &["cf_name"],
                registry
            )
            .unwrap(),
            rocksdb_batch_commit_latency_seconds: register_histogram_vec_with_registry!(
                "rocksdb_write_batch_commit_latency_seconds",
                "Rocksdb schema batch commit latency in seconds",
                &["db_name"],
                exponential_buckets(1e-6, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            rocksdb_batch_commit_bytes: register_histogram_vec_with_registry!(
                "rocksdb_batch_commit_bytes",
                "Rocksdb schema batch commit size in bytes",
                &["db_name"],
                registry,
            )
            .unwrap(),
        }
    }
}
