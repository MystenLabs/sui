// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, Registry,
};
use std::sync::Arc;

pub struct CheckpointMetrics {
    pub last_certified_checkpoint: IntGauge,
    pub last_constructed_checkpoint: IntGauge,
    pub checkpoint_errors: IntCounter,
    pub builder_utilization: IntCounter,
    pub aggregator_utilization: IntCounter,
    pub transactions_included_in_checkpoint: IntCounter,
    pub checkpoint_roots_count: IntCounter,
    pub checkpoint_participation: IntCounterVec,
}

impl CheckpointMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            last_certified_checkpoint: register_int_gauge_with_registry!(
                "last_certified_checkpoint",
                "Last certified checkpoint",
                registry
            )
            .unwrap(),
            last_constructed_checkpoint: register_int_gauge_with_registry!(
                "last_constructed_checkpoint",
                "Last constructed checkpoint",
                registry
            )
            .unwrap(),
            checkpoint_errors: register_int_counter_with_registry!(
                "checkpoint_errors",
                "Checkpoints errors count",
                registry
            )
            .unwrap(),
            builder_utilization: register_int_counter_with_registry!(
                "builder_utilization",
                "Checkpoints builder task utilization",
                registry
            )
            .unwrap(),
            aggregator_utilization: register_int_counter_with_registry!(
                "aggregator_utilization",
                "Checkpoints aggregator task utilization",
                registry
            )
            .unwrap(),
            transactions_included_in_checkpoint: register_int_counter_with_registry!(
                "transactions_included_in_checkpoint",
                "Transactions included in a checkpoint",
                registry
            )
            .unwrap(),
            checkpoint_roots_count: register_int_counter_with_registry!(
                "checkpoint_roots_count",
                "Number of checkpoint roots received from consensus",
                registry
            )
            .unwrap(),
            checkpoint_participation: register_int_counter_vec_with_registry!(
                "checkpoint_participation",
                "Participation in checkpoint certification by validator",
                &["signer"],
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }

    pub fn new_for_tests() -> Arc<Self> {
        Self::new(&Registry::new())
    }
}
