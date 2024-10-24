// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, IntCounter, IntCounterVec, IntGaugeVec, Registry,
};
use sui_indexer_builder::metrics::IndexerMetricProvider;

#[derive(Clone, Debug)]
pub struct DeepBookIndexerMetrics {
    pub(crate) total_deepbook_transactions: IntCounter,
    pub(crate) backfill_tasks_remaining_checkpoints: IntGaugeVec,
    pub(crate) tasks_processed_checkpoints: IntCounterVec,
    pub(crate) inflight_live_tasks: IntGaugeVec,
    pub(crate) tasks_latest_retrieved_checkpoints: IntGaugeVec,
}

impl DeepBookIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_deepbook_transactions: register_int_counter_with_registry!(
                "deepbook_indexer_total_deepbook_transactions",
                "Total number of deepbook transactions",
                registry,
            )
            .unwrap(),
            backfill_tasks_remaining_checkpoints: register_int_gauge_vec_with_registry!(
                "deepbook_indexer_backfill_tasks_remaining_checkpoints",
                "The remaining checkpoints for the currently running backfill task",
                &["task_name"],
                registry,
            )
            .unwrap(),
            tasks_processed_checkpoints: register_int_counter_vec_with_registry!(
                "deepbook_indexer_tasks_processed_checkpoints",
                "Total processed checkpoints for each task",
                &["task_name", "task_type"],
                registry,
            )
            .unwrap(),
            inflight_live_tasks: register_int_gauge_vec_with_registry!(
                "deepbook_indexer_inflight_live_tasks",
                "Number of inflight live tasks",
                &["task_name"],
                registry,
            )
            .unwrap(),
            tasks_latest_retrieved_checkpoints: register_int_gauge_vec_with_registry!(
                "deepbook_indexer_tasks_latest_retrieved_checkpoints",
                "latest retrieved checkpoint for each task",
                &["task_name", "task_type"],
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

impl IndexerMetricProvider for DeepBookIndexerMetrics {
    fn get_tasks_latest_retrieved_checkpoints(&self) -> &IntGaugeVec {
        &self.tasks_latest_retrieved_checkpoints
    }

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec {
        &self.backfill_tasks_remaining_checkpoints
    }

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec {
        &self.tasks_processed_checkpoints
    }

    fn get_inflight_live_tasks_metrics(&self) -> &IntGaugeVec {
        &self.inflight_live_tasks
    }
}
