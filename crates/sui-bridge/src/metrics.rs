// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Registry,
};

#[derive(Clone)]
pub struct BridgeMetrics {
    pub(crate) err_build_sui_transaction: IntCounter,
    pub(crate) err_signature_aggregation: IntCounter,
    pub(crate) err_sui_transaction_submission: IntCounter,
    pub(crate) err_sui_transaction_submission_too_many_failures: IntCounter,
    pub(crate) err_sui_transaction_execution: IntCounter,
    pub(crate) requests_received: IntCounterVec,
    pub(crate) requests_ok: IntCounterVec,
    pub(crate) err_requests: IntCounterVec,
    pub(crate) requests_inflight: IntGaugeVec,

    pub last_synced_sui_checkpoint: IntGauge,
    pub(crate) last_finalized_eth_block: IntGauge,
    pub(crate) last_synced_eth_block: IntGauge,

    pub(crate) sui_watcher_received_events: IntCounter,
    pub(crate) sui_watcher_received_actions: IntCounter,
    pub(crate) sui_watcher_unrecognized_events: IntCounter,
    pub(crate) eth_watcher_received_events: IntCounter,
    pub(crate) eth_watcher_received_actions: IntCounter,
    pub(crate) eth_watcher_unrecognized_events: IntCounter,
    pub(crate) action_executor_already_processed_actions: IntCounter,
    pub(crate) action_executor_signing_queue_received_actions: IntCounter,
    pub(crate) action_executor_signing_queue_skipped_actions: IntCounter,
    pub(crate) action_executor_execution_queue_received_actions: IntCounter,

    pub(crate) signer_with_cache_hit: IntCounterVec,
    pub(crate) signer_with_cache_miss: IntCounterVec,

    pub(crate) eth_provider_queries: IntCounter,
    pub(crate) gas_coin_balance: IntGauge,
}

impl BridgeMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            err_build_sui_transaction: register_int_counter_with_registry!(
                "bridge_err_build_sui_transaction",
                "Total number of errors of building sui transactions",
                registry,
            )
            .unwrap(),
            err_signature_aggregation: register_int_counter_with_registry!(
                "bridge_err_signature_aggregation",
                "Total number of errors of aggregating validators signatures",
                registry,
            )
            .unwrap(),
            err_sui_transaction_submission: register_int_counter_with_registry!(
                "bridge_err_sui_transaction_submission",
                "Total number of errors of submitting sui transactions",
                registry,
            )
            .unwrap(),
            err_sui_transaction_submission_too_many_failures: register_int_counter_with_registry!(
                "bridge_err_sui_transaction_submission_too_many_failures",
                "Total number of continuous failures to submitting sui transactions",
                registry,
            )
            .unwrap(),
            err_sui_transaction_execution: register_int_counter_with_registry!(
                "bridge_err_sui_transaction_execution",
                "Total number of failures of sui transaction execution",
                registry,
            )
            .unwrap(),
            requests_received: register_int_counter_vec_with_registry!(
                "bridge_requests_received",
                "Total number of requests received in Server, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            requests_ok: register_int_counter_vec_with_registry!(
                "bridge_requests_ok",
                "Total number of ok requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            err_requests: register_int_counter_vec_with_registry!(
                "bridge_err_requests",
                "Total number of erred requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            requests_inflight: register_int_gauge_vec_with_registry!(
                "bridge_requests_inflight",
                "Total number of inflight requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            sui_watcher_received_events: register_int_counter_with_registry!(
                "bridge_sui_watcher_received_events",
                "Total number of received events in sui watcher",
                registry,
            )
            .unwrap(),
            eth_watcher_received_events: register_int_counter_with_registry!(
                "bridge_eth_watcher_received_events",
                "Total number of received events in eth watcher",
                registry,
            )
            .unwrap(),
            sui_watcher_received_actions: register_int_counter_with_registry!(
                "bridge_sui_watcher_received_actions",
                "Total number of received actions in sui watcher",
                registry,
            )
            .unwrap(),
            eth_watcher_received_actions: register_int_counter_with_registry!(
                "bridge_eth_watcher_received_actions",
                "Total number of received actions in eth watcher",
                registry,
            )
            .unwrap(),
            sui_watcher_unrecognized_events: register_int_counter_with_registry!(
                "bridge_sui_watcher_unrecognized_events",
                "Total number of unrecognized events in sui watcher",
                registry,
            )
            .unwrap(),
            eth_watcher_unrecognized_events: register_int_counter_with_registry!(
                "bridge_eth_watcher_unrecognized_events",
                "Total number of unrecognized events in eth watcher",
                registry,
            )
            .unwrap(),
            action_executor_already_processed_actions: register_int_counter_with_registry!(
                "bridge_action_executor_already_processed_actions",
                "Total number of already processed actions action executor",
                registry,
            )
            .unwrap(),
            action_executor_signing_queue_received_actions: register_int_counter_with_registry!(
                "bridge_action_executor_signing_queue_received_actions",
                "Total number of received actions in action executor signing queue",
                registry,
            )
            .unwrap(),
            action_executor_signing_queue_skipped_actions: register_int_counter_with_registry!(
                "bridge_action_executor_signing_queue_skipped_actions",
                "Total number of skipped actions in action executor signing queue",
                registry,
            )
            .unwrap(),
            action_executor_execution_queue_received_actions: register_int_counter_with_registry!(
                "bridge_action_executor_execution_queue_received_actions",
                "Total number of received actions in action executor execution queue",
                registry,
            )
            .unwrap(),
            gas_coin_balance: register_int_gauge_with_registry!(
                "bridge_gas_coin_balance",
                "Current balance of gas coin, in mist",
                registry,
            )
            .unwrap(),
            eth_provider_queries: register_int_counter_with_registry!(
                "bridge_eth_provider_queries",
                "Total number of queries issued to eth provider",
                registry,
            )
            .unwrap(),
            last_synced_sui_checkpoint: register_int_gauge_with_registry!(
                "last_synced_sui_checkpoint",
                "The latest sui checkpoint that indexer synced",
                registry,
            )
            .unwrap(),
            last_synced_eth_block: register_int_gauge_with_registry!(
                "bridge_last_synced_eth_block",
                "The latest finalized eth block that indexer synced",
                registry,
            )
            .unwrap(),
            last_finalized_eth_block: register_int_gauge_with_registry!(
                "bridge_last_finalized_eth_block",
                "The latest finalized eth block that indexer observed",
                registry,
            )
            .unwrap(),
            signer_with_cache_hit: register_int_counter_vec_with_registry!(
                "bridge_signer_with_cache_hit",
                "Total number of hit in signer's cache, by verifier type",
                &["type"],
                registry,
            )
            .unwrap(),
            signer_with_cache_miss: register_int_counter_vec_with_registry!(
                "bridge_signer_with_cache_miss",
                "Total number of miss in signer's cache, by verifier type",
                &["type"],
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
