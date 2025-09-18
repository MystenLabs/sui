// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::COUNT_BUCKETS;
use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, Registry,
};

const SUBMIT_TRANSACTION_RETRIES_BUCKETS: &[f64] = &[
    0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 15.0, 20.0, 30.0,
];

// TODO(mysticeti-fastpath): For validator names, use display name instead of concise name.
#[derive(Clone)]
pub struct TransactionDriverMetrics {
    pub(crate) settlement_finality_latency: HistogramVec,
    pub(crate) total_transactions_submitted: IntCounter,
    pub(crate) submit_transaction_retries: Histogram,
    pub(crate) submit_transaction_latency: Histogram,
    pub(crate) validator_submit_transaction_errors: IntCounterVec,
    pub(crate) validator_submit_transaction_successes: IntCounterVec,
    pub(crate) executed_transactions: IntCounter,
    pub(crate) rejection_acks: IntCounter,
    pub(crate) expiration_acks: IntCounter,
    pub(crate) effects_digest_mismatches: IntCounter,
    pub(crate) transaction_retries: HistogramVec,
    pub(crate) transaction_fastpath_acked: IntCounterVec,
    pub(crate) certified_effects_ack_latency: HistogramVec,
    pub(crate) certified_effects_ack_attempts: IntCounterVec,
    pub(crate) certified_effects_ack_successes: IntCounterVec,
    pub(crate) validator_selections: IntCounterVec,
    pub(crate) submit_amplification_factor: Histogram,
}

impl TransactionDriverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            settlement_finality_latency: register_histogram_vec_with_registry!(
                "transaction_driver_settlement_finality_latency",
                "Settlement finality latency observed from transaction driver",
                &["tx_type"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_transactions_submitted: register_int_counter_with_registry!(
                "transaction_driver_total_transactions_submitted",
                "Total number of transactions submitted through the transaction driver",
                registry,
            )
            .unwrap(),
            submit_transaction_retries: register_histogram_with_registry!(
                "transaction_driver_submit_transaction_retries",
                "Number of retries needed for successful transaction submission",
                SUBMIT_TRANSACTION_RETRIES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            submit_transaction_latency: register_histogram_with_registry!(
                "transaction_driver_submit_transaction_latency",
                "Time in seconds to successfully submit a transaction to a validator.\n\
                Includes all retries and measures from the start of submission\n\
                until a validator accepts the transaction.",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            validator_submit_transaction_errors: register_int_counter_vec_with_registry!(
                "transaction_driver_validator_submit_transaction_errors",
                "Number of submit transaction errors by validator",
                &["validator", "error_type"],
                registry,
            )
            .unwrap(),
            validator_submit_transaction_successes: register_int_counter_vec_with_registry!(
                "transaction_driver_validator_submit_transaction_successes",
                "Number of successful submit transactions by validator",
                &["validator"],
                registry,
            )
            .unwrap(),
            executed_transactions: register_int_counter_with_registry!(
                "transaction_driver_executed_transactions",
                "Number of transactions executed observed by the transaction driver",
                registry,
            )
            .unwrap(),
            rejection_acks: register_int_counter_with_registry!(
                "transaction_driver_rejected_acks",
                "Number of rejection acknowledgments observed by the transaction driver",
                registry,
            )
            .unwrap(),
            expiration_acks: register_int_counter_with_registry!(
                "transaction_driver_expiration_acks",
                "Number of expiration acknowledgments observed by the transaction driver",
                registry,
            )
            .unwrap(),
            effects_digest_mismatches: register_int_counter_with_registry!(
                "transaction_driver_effects_digest_mismatches",
                "Number of effects digest mismatches detected by the transaction driver",
                registry,
            )
            .unwrap(),
            transaction_retries: register_histogram_vec_with_registry!(
                "transaction_driver_transaction_retries",
                "Number of retries per transaction attempt in drive_transaction",
                &["result"],
                SUBMIT_TRANSACTION_RETRIES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            transaction_fastpath_acked: register_int_counter_vec_with_registry!(
                "transaction_driver_transaction_fastpath_acked",
                "Number of transactions that were executed using fast path",
                &["validator"],
                registry,
            )
            .unwrap(),
            certified_effects_ack_latency: register_histogram_vec_with_registry!(
                "transaction_driver_certified_effects_ack_latency",
                "Latency in seconds for getting certified effects acknowledgment",
                &["tx_type"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            certified_effects_ack_attempts: register_int_counter_vec_with_registry!(
                "transaction_driver_certified_effects_ack_attempts",
                "Total number of transactions that went through certified effects ack process",
                &["tx_type"],
                registry,
            )
            .unwrap(),
            certified_effects_ack_successes: register_int_counter_vec_with_registry!(
                "transaction_driver_certified_effects_ack_successes",
                "Number of successful certified effects acknowledgments",
                &["tx_type"],
                registry,
            )
            .unwrap(),
            validator_selections: register_int_counter_vec_with_registry!(
                "transaction_driver_validator_selections",
                "Number of times each validator was selected for transaction submission",
                &["validator"],
                registry,
            )
            .unwrap(),
            submit_amplification_factor: register_histogram_with_registry!(
                "transaction_driver_submit_amplification_factor",
                "The amplification factor used by transaction driver to submit to validators",
                COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}
