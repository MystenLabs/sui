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

    pub(crate) sui_watcher_received_events: IntCounter,
    pub(crate) sui_watcher_received_actions: IntCounter,
    pub(crate) sui_watcher_unrecognized_events: IntCounter,
    pub(crate) eth_watcher_received_events: IntCounter,
    pub(crate) eth_watcher_received_actions: IntCounter,
    pub(crate) eth_watcher_unrecognized_events: IntCounter,
    pub(crate) action_executor_signing_queue_received_actions: IntCounter,
    pub(crate) action_executor_execution_queue_received_actions: IntCounter,

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
            action_executor_signing_queue_received_actions: register_int_counter_with_registry!(
                "bridge_action_executor_signing_queue_received_actions",
                "Total number of received actions in action executor signing queue",
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
        }
    }

    pub fn new_for_testing() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}
