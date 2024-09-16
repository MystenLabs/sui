// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::MetricsConfig;
use mysten_common::metrics::{push_metrics, MetricsPushClient};
use mysten_metrics::RegistryService;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Registry,
};
use std::time::Duration;
use sui_types::crypto::NetworkKeyPair;
use tokio::time::sleep;

const FINE_GRAINED_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.6, 0.7, 0.8, 0.9,
    1.0, 1.2, 1.4, 1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5,
    10., 15., 20., 25., 30., 35., 40., 45., 50., 60., 70., 80., 90., 100., 120., 140., 160., 180.,
    200., 250., 300., 350., 400.,
];

/// Starts a task to periodically push metrics to a configured endpoint if a metrics push endpoint
/// is configured.
pub fn start_metrics_push_task(
    metrics_config: &Option<MetricsConfig>,
    metrics_key_pair: NetworkKeyPair,
    registry: RegistryService,
) {
    use fastcrypto::traits::KeyPair;

    const DEFAULT_METRICS_PUSH_INTERVAL: Duration = Duration::from_secs(60);

    let (interval, url) = match metrics_config {
        Some(MetricsConfig {
            push_interval_seconds,
            push_url: url,
        }) => {
            let interval = push_interval_seconds
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_METRICS_PUSH_INTERVAL);
            let url = reqwest::Url::parse(url).expect("unable to parse metrics push url");
            (interval, url)
        }
        _ => return,
    };

    let mut client = MetricsPushClient::new(metrics_key_pair.copy());

    tokio::spawn(async move {
        tracing::info!(push_url =% url, interval =? interval, "Started Metrics Push Service");

        let mut interval = tokio::time::interval(interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            // Retry pushing metrics if there is an error.
            while let Err(error) = push_metrics(&client, &url, &registry).await {
                tracing::warn!("unable to push metrics: {error}; new client will be created");
                sleep(Duration::from_secs(1)).await;
                // aggressively recreate our client connection if we hit an error
                client = MetricsPushClient::new(metrics_key_pair.copy());
            }
        }
    });
}

#[derive(Clone, Debug)]
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
    pub(crate) action_executor_execution_queue_skipped_actions_due_to_pausing: IntCounter,

    pub(crate) signer_with_cache_hit: IntCounterVec,
    pub(crate) signer_with_cache_miss: IntCounterVec,

    pub(crate) eth_rpc_queries: IntCounterVec,
    pub(crate) eth_rpc_queries_latency: HistogramVec,

    pub(crate) gas_coin_balance: IntGauge,

    pub(crate) sui_rpc_errors: IntCounterVec,
    pub(crate) observed_governance_actions: IntCounterVec,
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
            action_executor_execution_queue_skipped_actions_due_to_pausing: register_int_counter_with_registry!(
                "bridge_action_executor_execution_queue_skipped_actions_due_to_pausing",
                "Total number of skipped actions in action executor execution queue because of pausing",
                registry,
            )
            .unwrap(),
            gas_coin_balance: register_int_gauge_with_registry!(
                "bridge_gas_coin_balance",
                "Current balance of gas coin, in mist",
                registry,
            )
            .unwrap(),
            eth_rpc_queries: register_int_counter_vec_with_registry!(
                "bridge_eth_rpc_queries",
                "Total number of queries issued to eth provider, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            eth_rpc_queries_latency: register_histogram_vec_with_registry!(
                "bridge_eth_rpc_queries_latency",
                "Latency of queries issued to eth provider, by request type",
                &["type"],
                FINE_GRAINED_LATENCY_SEC_BUCKETS.to_vec(),
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
            sui_rpc_errors: register_int_counter_vec_with_registry!(
                "bridge_sui_rpc_errors",
                "Total number of errors from sui RPC, by RPC method",
                &["method"],
                registry,
            )
            .unwrap(),
            observed_governance_actions: register_int_counter_vec_with_registry!(
                "bridge_observed_governance_actions",
                "Total number of observed governance actions",
                &["action_type", "chain_id"],
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
