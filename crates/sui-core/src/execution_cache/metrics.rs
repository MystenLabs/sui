// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::trace;

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, Registry,
};

pub struct ExecutionCacheMetrics {
    pub(crate) pending_notify_read: IntGauge,
    pub(crate) cache_requests: IntCounterVec,
    pub(crate) cache_hits: IntCounterVec,
    pub(crate) cache_negative_hits: IntCounterVec,
    pub(crate) cache_misses: IntCounterVec,
    pub(crate) cache_writes: IntCounterVec,
    pub(crate) expired_tickets: IntCounter,
    pub(crate) backpressure_status: IntGauge,
    pub(crate) backpressure_toggles: IntCounter,
}

impl ExecutionCacheMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            pending_notify_read: register_int_gauge_with_registry!(
                "pending_notify_read",
                "Pending notify read requests",
                registry,
            )
            .unwrap(),
            // `request_type` is "object_by_version", "object_latest", "transaction", etc
            // level in these metrics may be "uncommitted", "committed", "package_cache" or "db"
            cache_requests: register_int_counter_vec_with_registry!(
                "execution_cache_requests",
                "Execution cache requests",
                &["request_type", "level"],
                registry,
            )
            .unwrap(),
            cache_hits: register_int_counter_vec_with_registry!(
                "execution_cache_hits",
                "Execution cache hits",
                &["request_type", "level"],
                registry,
            )
            .unwrap(),
            cache_negative_hits: register_int_counter_vec_with_registry!(
                "execution_cache_negative_hits",
                "Execution cache negative hits",
                &["request_type", "level"],
                registry,
            )
            .unwrap(),
            cache_misses: register_int_counter_vec_with_registry!(
                "execution_cache_misses",
                "Execution cache misses",
                &["request_type", "level"],
                registry,
            )
            .unwrap(),

            // `collection` should be "object", "marker", "transaction_effects", etc
            cache_writes: register_int_counter_vec_with_registry!(
                "execution_cache_writes",
                "Execution cache writes",
                &["collection"],
                registry,
            )
            .unwrap(),

            expired_tickets: register_int_counter_with_registry!(
                "execution_cache_expired_tickets",
                "Failed inserts to monotonic caches because of expired tickets",
                registry,
            )
            .unwrap(),
            backpressure_status: register_int_gauge_with_registry!(
                "execution_cache_backpressure_status",
                "Backpressure status (1 = on, 0 = off)",
                registry,
            )
            .unwrap(),
            backpressure_toggles: register_int_counter_with_registry!(
                "execution_cache_backpressure_toggles",
                "Number of times backpressure was turned on or off",
                registry,
            )
            .unwrap(),
        }
    }

    pub(crate) fn record_cache_request(&self, request_type: &'static str, level: &'static str) {
        trace!(target: "cache_metrics", "Cache request: {} {}", request_type, level);
        self.cache_requests
            .with_label_values(&[request_type, level])
            .inc();
    }

    pub(crate) fn record_cache_multi_request(
        &self,
        request_type: &'static str,
        level: &'static str,
        count: usize,
    ) {
        trace!(
            target: "cache_metrics",
            "Cache multi request: {} {} count: {}",
            request_type,
            level,
            count
        );
        self.cache_requests
            .with_label_values(&[request_type, level])
            .inc_by(count as u64);
    }

    pub(crate) fn record_cache_hit(&self, request_type: &'static str, level: &'static str) {
        trace!(target: "cache_metrics", "Cache hit: {} {}", request_type, level);
        self.cache_hits
            .with_label_values(&[request_type, level])
            .inc();
    }

    pub(crate) fn record_cache_miss(&self, request_type: &'static str, level: &'static str) {
        trace!(target: "cache_metrics", "Cache miss: {} {}", request_type, level);
        self.cache_misses
            .with_label_values(&[request_type, level])
            .inc();
    }

    pub(crate) fn record_cache_negative_hit(
        &self,
        request_type: &'static str,
        level: &'static str,
    ) {
        trace!(target: "cache_metrics", "Cache negative hit: {} {}", request_type, level);
        self.cache_negative_hits
            .with_label_values(&[request_type, level])
            .inc();
    }

    pub(crate) fn record_cache_write(&self, collection: &'static str) {
        self.cache_writes.with_label_values(&[collection]).inc();
    }

    pub(crate) fn record_ticket_expiry(&self) {
        self.expired_tickets.inc();
    }
}
