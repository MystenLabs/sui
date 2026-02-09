// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    Histogram, IntCounterVec, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_with_registry,
};

const SCHEDULING_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
];

pub struct ObjectFundsCheckerMetrics {
    pub highest_settled_version: IntGauge,
    pub pending_checks: IntGauge,
    pub pending_check_latency: Histogram,
    pub unsettled_accounts: IntGauge,
    pub unsettled_versions: IntGauge,
    pub check_result: IntCounterVec,
}

impl ObjectFundsCheckerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            highest_settled_version: register_int_gauge_with_registry!(
                "object_funds_highest_settled_version",
                "Highest settled accumulator version",
                registry,
            )
            .unwrap(),
            pending_checks: register_int_gauge_with_registry!(
                "object_funds_pending_checks",
                "Number of pending unresolved object funds checks",
                registry,
            )
            .unwrap(),
            pending_check_latency: register_histogram_with_registry!(
                "object_funds_pending_check_latency",
                "Latency in seconds from pending check creation to resolution",
                SCHEDULING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            unsettled_accounts: register_int_gauge_with_registry!(
                "object_funds_unsettled_accounts",
                "Number of accounts with unsettled withdraws",
                registry,
            )
            .unwrap(),
            unsettled_versions: register_int_gauge_with_registry!(
                "object_funds_unsettled_versions",
                "Number of versions with unsettled accounts",
                registry,
            )
            .unwrap(),
            check_result: register_int_counter_vec_with_registry!(
                "object_funds_check_result",
                "Count of object funds check results by outcome",
                &["result"],
                registry,
            )
            .unwrap(),
        }
    }
}
