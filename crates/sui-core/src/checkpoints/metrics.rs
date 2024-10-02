// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::histogram::Histogram as MystenHistogram;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry,
};
use std::sync::Arc;

pub struct CheckpointMetrics {
    pub last_certified_checkpoint: IntGauge,
    pub last_constructed_checkpoint: IntGauge,
    pub checkpoint_errors: IntCounter,
    pub transactions_included_in_checkpoint: IntCounter,
    pub checkpoint_roots_count: IntCounter,
    pub checkpoint_participation: IntCounterVec,
    pub last_received_checkpoint_signatures: IntGaugeVec,
    pub last_sent_checkpoint_signature: IntGauge,
    pub last_skipped_checkpoint_signature_submission: IntGauge,
    pub last_ignored_checkpoint_signature_received: IntGauge,
    pub highest_accumulated_epoch: IntGauge,
    pub checkpoint_creation_latency: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    pub checkpoint_creation_latency_ms: MystenHistogram,
    pub remote_checkpoint_forks: IntCounter,
    pub split_brain_checkpoint_forks: IntCounter,
    pub last_created_checkpoint_age: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    pub last_created_checkpoint_age_ms: MystenHistogram,
    pub last_certified_checkpoint_age: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    pub last_certified_checkpoint_age_ms: MystenHistogram,
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
            last_created_checkpoint_age: register_histogram_with_registry!(
                "last_created_checkpoint_age",
                "Age of the last created checkpoint",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            last_created_checkpoint_age_ms: MystenHistogram::new_in_registry(
                "last_created_checkpoint_age_ms",
                "Age of the last created checkpoint",
                registry
            ),
            last_certified_checkpoint_age: register_histogram_with_registry!(
                "last_certified_checkpoint_age",
                "Age of the last certified checkpoint",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            last_certified_checkpoint_age_ms: MystenHistogram::new_in_registry(
                "last_certified_checkpoint_age_ms",
                "Age of the last certified checkpoint",
                registry
            ),
            checkpoint_errors: register_int_counter_with_registry!(
                "checkpoint_errors",
                "Checkpoints errors count",
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
            last_received_checkpoint_signatures: register_int_gauge_vec_with_registry!(
                "last_received_checkpoint_signatures",
                "Last received checkpoint signatures by validator",
                &["signer"],
                registry
            )
            .unwrap(),
            last_sent_checkpoint_signature: register_int_gauge_with_registry!(
                "last_sent_checkpoint_signature",
                "Last checkpoint signature sent by myself",
                registry
            )
            .unwrap(),
            last_skipped_checkpoint_signature_submission: register_int_gauge_with_registry!(
                "last_skipped_checkpoint_signature_submission",
                "Last checkpoint signature that this validator skipped submitting because it was already certfied.",
                registry
            )
            .unwrap(),
            last_ignored_checkpoint_signature_received: register_int_gauge_with_registry!(
                "last_ignored_checkpoint_signature_received",
                "Last received checkpoint signature that this validator ignored because it was already certfied.",
                registry
            )
            .unwrap(),
            highest_accumulated_epoch: register_int_gauge_with_registry!(
                "highest_accumulated_epoch",
                "Highest accumulated epoch",
                registry
            )
            .unwrap(),
            checkpoint_creation_latency: register_histogram_with_registry!(
                "checkpoint_creation_latency",
                "Latency from consensus commit timstamp to local checkpoint creation in milliseconds",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            checkpoint_creation_latency_ms: MystenHistogram::new_in_registry(
                "checkpoint_creation_latency_ms",
                "Latency from consensus commit timstamp to local checkpoint creation in milliseconds",
                registry,
            ),
            remote_checkpoint_forks: register_int_counter_with_registry!(
                "remote_checkpoint_forks",
                "Number of remote checkpoints that forked from local checkpoints",
                registry
            )
            .unwrap(),
            split_brain_checkpoint_forks: register_int_counter_with_registry!(
                "split_brain_checkpoint_forks",
                "Number of checkpoints that have resulted in a split brain",
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
