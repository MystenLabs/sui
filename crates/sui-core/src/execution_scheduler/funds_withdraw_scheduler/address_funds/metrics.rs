// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    Histogram, IntCounter, IntCounterVec, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};

const SCHEDULING_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
];

pub(crate) struct AddressFundsSchedulerMetrics {
    pub highest_scheduled_version: IntGauge,
    pub highest_settled_version: IntGauge,
    pub pending_schedules: IntGauge,
    pub pending_schedule_latency: Histogram,
    pub tracked_accounts: IntGauge,
    pub total_scheduled: IntCounter,
    pub total_settled: IntCounter,
    pub schedule_result: IntCounterVec,
}

impl AddressFundsSchedulerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            highest_scheduled_version: register_int_gauge_with_registry!(
                "address_funds_highest_scheduled_version",
                "Highest accumulator version seen in schedule_withdraws",
                registry,
            )
            .unwrap(),
            highest_settled_version: register_int_gauge_with_registry!(
                "address_funds_highest_settled_version",
                "Highest accumulator version settled via settle_funds",
                registry,
            )
            .unwrap(),
            pending_schedules: register_int_gauge_with_registry!(
                "address_funds_pending_schedules",
                "In-flight pending schedules not yet resolved",
                registry,
            )
            .unwrap(),
            pending_schedule_latency: register_histogram_with_registry!(
                "address_funds_pending_schedule_latency",
                "Latency in seconds from pending schedule creation to resolution",
                SCHEDULING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            tracked_accounts: register_int_gauge_with_registry!(
                "address_funds_tracked_accounts",
                "Number of accounts being tracked by the eager scheduler",
                registry,
            )
            .unwrap(),
            total_scheduled: register_int_counter_with_registry!(
                "address_funds_total_scheduled",
                "Total withdrawal transactions scheduled",
                registry,
            )
            .unwrap(),
            total_settled: register_int_counter_with_registry!(
                "address_funds_total_settled",
                "Total settlement versions processed",
                registry,
            )
            .unwrap(),
            schedule_result: register_int_counter_vec_with_registry!(
                "address_funds_schedule_result",
                "Count of address funds scheduling decisions by outcome",
                &["result"],
                registry,
            )
            .unwrap(),
        }
    }
}
