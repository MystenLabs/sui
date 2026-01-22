// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    IntCounter, IntCounterVec, IntGaugeVec, Registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
};

pub trait IndexerMetricProvider: Send + Sync {
    fn get_tasks_latest_retrieved_checkpoints(&self) -> &IntGaugeVec;

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec;

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec;

    fn get_inflight_live_tasks_metrics(&self) -> &IntGaugeVec;

    fn boxed(self) -> Box<dyn IndexerMetricProvider>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

#[derive(Clone, Debug)]
pub struct BridgeIndexerMetrics {
    pub(crate) total_eth_bridge_transactions: IntCounter,
    pub(crate) total_eth_token_deposited: IntCounter,
    pub(crate) total_eth_token_transfer_claimed: IntCounter,
    pub(crate) backfill_tasks_remaining_checkpoints: IntGaugeVec,
    pub(crate) tasks_processed_checkpoints: IntCounterVec,
    pub(crate) tasks_latest_retrieved_checkpoints: IntGaugeVec,
    pub(crate) inflight_live_tasks: IntGaugeVec,
}

impl BridgeIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_eth_bridge_transactions: register_int_counter_with_registry!(
                "bridge_indexer_total_eth_bridge_transactions",
                "Total number of eth bridge transactions",
                registry,
            )
            .unwrap(),
            total_eth_token_deposited: register_int_counter_with_registry!(
                "bridge_indexer_total_eth_token_deposited",
                "Total number of eth token deposited transactions",
                registry,
            )
            .unwrap(),
            total_eth_token_transfer_claimed: register_int_counter_with_registry!(
                "bridge_indexer_total_eth_token_transfer_claimed",
                "Total number of eth token claimed transactions",
                registry,
            )
            .unwrap(),
            backfill_tasks_remaining_checkpoints: register_int_gauge_vec_with_registry!(
                "bridge_indexer_backfill_tasks_remaining_checkpoints",
                "The remaining checkpoints for the currently running backfill task",
                &["task_name"],
                registry,
            )
            .unwrap(),
            tasks_processed_checkpoints: register_int_counter_vec_with_registry!(
                "bridge_indexer_tasks_processed_checkpoints",
                "Total processed checkpoints for each task",
                &["task_name", "task_type"],
                registry,
            )
            .unwrap(),
            tasks_latest_retrieved_checkpoints: register_int_gauge_vec_with_registry!(
                "bridge_indexer_tasks_latest_retrieved_checkpoints",
                "latest retrieved checkpoint for each task",
                &["task_name", "task_type"],
                registry,
            )
            .unwrap(),
            inflight_live_tasks: register_int_gauge_vec_with_registry!(
                "bridge_indexer_inflight_live_tasks",
                "Number of inflight live tasks",
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

impl IndexerMetricProvider for BridgeIndexerMetrics {
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
