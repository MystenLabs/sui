// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, IntCounter, IntCounterVec, IntGaugeVec, Registry,
};

#[derive(Clone, Debug)]
pub struct DeepBookIndexerMetrics {
    pub(crate) total_deepbook_transactions: IntCounter,
    pub(crate) tasks_remaining_checkpoints: IntGaugeVec,
    pub(crate) tasks_processed_checkpoints: IntCounterVec,
}

impl DeepBookIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_deepbook_transactions: register_int_counter_with_registry!(
                "total_deepbook_transactions",
                "Total number of deepbook transactions",
                registry,
            )
            .unwrap(),
            tasks_remaining_checkpoints: register_int_gauge_vec_with_registry!(
                "bridge_indexer_tasks_remaining_checkpoints",
                "The remaining checkpoints for each task",
                &["task_name"],
                registry,
            )
            .unwrap(),
            tasks_processed_checkpoints: register_int_counter_vec_with_registry!(
                "bridge_indexer_tasks_processed_checkpoints",
                "Total processed checkpoints for each task",
                &["task_name"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_testing() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}
