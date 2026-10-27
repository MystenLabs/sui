// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    IntCounterVec, IntGauge, Registry, register_int_counter_vec_with_registry,
    register_int_gauge_with_registry,
};

pub struct ObjectFundsCheckerMetrics {
    pub unsettled_accounts: IntGauge,
    pub unsettled_versions: IntGauge,
    pub in_execution_check_result: IntCounterVec,
}

impl ObjectFundsCheckerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
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
            in_execution_check_result: register_int_counter_vec_with_registry!(
                "object_funds_in_execution_check_result",
                "Count of committed in-execution object funds check outcomes by result",
                &["result"],
                registry,
            )
            .unwrap(),
        }
    }
}
