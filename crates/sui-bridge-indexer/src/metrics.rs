// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
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
    pub(crate) last_committed_eth_block: IntGauge,
    pub(crate) last_synced_unfinalized_eth_block: IntGauge,
    pub(crate) last_committed_unfinalized_eth_block: IntGauge,
}

impl BridgeIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_sui_bridge_transactions: register_int_counter_with_registry!(
                "total_sui_bridge_transactions",
                "Total number of sui bridge transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_deposited: register_int_counter_with_registry!(
                "total_sui_token_deposited",
                "Total number of sui token deposited transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_transfer_approved: register_int_counter_with_registry!(
                "total_sui_token_transfer_approved",
                "Total number of sui token approved transactions",
                registry,
            )
            .unwrap(),
            total_sui_token_transfer_claimed: register_int_counter_with_registry!(
                "total_sui_token_transfer_claimed",
                "Total number of sui token claimed transactions",
                registry,
            )
            .unwrap(),
            total_sui_bridge_txn_other: register_int_counter_with_registry!(
                "total_sui_bridge_txn_other",
                "Total number of other sui bridge transactions",
                registry,
            )
            .unwrap(),
            total_eth_bridge_transactions: register_int_counter_with_registry!(
                "total_eth_bridge_transactions",
                "Total number of eth bridge transactions",
                registry,
            )
            .unwrap(),
            total_eth_token_deposited: register_int_counter_with_registry!(
                "total_eth_token_deposited",
                "Total number of eth token deposited transactions",
                registry,
            )
            .unwrap(),
            total_eth_token_transfer_claimed: register_int_counter_with_registry!(
                "total_eth_token_transfer_claimed",
                "Total number of eth token claimed transactions",
                registry,
            )
            .unwrap(),
            total_eth_bridge_txn_other: register_int_counter_with_registry!(
                "total_eth_bridge_txn_other",
                "Total number of other eth bridge transactions",
                registry,
            )
            .unwrap(),
            last_committed_sui_checkpoint: register_int_gauge_with_registry!(
                "last_committed_sui_checkpoint",
                "The latest sui checkpoint that indexer committed to DB",
                registry,
            )
            .unwrap(),
            last_committed_eth_block: register_int_gauge_with_registry!(
                "last_committed_eth_block",
                "The latest eth block that indexer committed to DB",
                registry,
            )
            .unwrap(),
            last_synced_unfinalized_eth_block: register_int_gauge_with_registry!(
                "last_synced_unfinalized_eth_block",
                "The latest unfinalized block that indexer synced",
                registry,
            )
            .unwrap(),
            last_committed_unfinalized_eth_block: register_int_gauge_with_registry!(
                "last_committed_unfinalized_eth_block",
                "The latest unfinalized block that indexer comitted to DB",
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
