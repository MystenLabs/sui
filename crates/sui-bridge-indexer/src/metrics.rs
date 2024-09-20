// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Registry,
};

#[derive(Clone, Debug)]
pub struct BridgeIndexerMetrics {
    pub(crate) total_sui_bridge_transactions: IntCounter,
    pub(crate) total_sui_token_deposited: IntCounter,
    pub(crate) total_sui_token_transfer_approved: IntCounter,
    pub(crate) total_sui_token_transfer_claimed: IntCounter,
    pub(crate) total_sui_bridge_txn_other: IntCounter,
    pub(crate) total_eth_bridge_transactions: IntCounter,
    pub(crate) total_eth_token_deposited: IntCounter,
    pub(crate) total_eth_token_transfer_claimed: IntCounter,
    pub(crate) total_eth_bridge_txn_other: IntCounter,
    pub(crate) last_committed_sui_checkpoint: IntGauge,
    pub(crate) backfill_tasks_remaining_checkpoints: IntGaugeVec,
    pub(crate) tasks_processed_checkpoints: IntCounterVec,
    pub(crate) tasks_current_checkpoints: IntGaugeVec,
}

impl BridgeIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_sui_bridge_transactions: register_int_counter_with_registry!(
                "bridge_indexer_total_sui_bridge_transactions",
                "Total number of sui bridge transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_deposited: register_int_counter_with_registry!(
                "bridge_indexer_total_sui_token_deposited",
                "Total number of sui token deposited transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_transfer_approved: register_int_counter_with_registry!(
                "bridge_indexer_total_sui_token_transfer_approved",
                "Total number of sui token approved transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_transfer_claimed: register_int_counter_with_registry!(
                "bridge_indexer_total_sui_token_transfer_claimed",
                "Total number of sui token claimed transactions",
                registry,
            )
            .unwrap(),
            total_sui_bridge_txn_other: register_int_counter_with_registry!(
                "bridge_indexer_total_sui_bridge_txn_other",
                "Total number of other sui bridge transactions",
                registry,
            )
            .unwrap(),
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
            total_eth_bridge_txn_other: register_int_counter_with_registry!(
                "bridge_indexer_total_eth_bridge_txn_other",
                "Total number of other eth bridge transactions",
                registry,
            )
            .unwrap(),
            last_committed_sui_checkpoint: register_int_gauge_with_registry!(
                "bridge_indexer_last_committed_sui_checkpoint",
                "The latest sui checkpoint that indexer committed to DB",
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
            tasks_current_checkpoints: register_int_gauge_vec_with_registry!(
                "bridge_indexer_tasks_current_checkpoints",
                "Current checkpoint for each task",
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
