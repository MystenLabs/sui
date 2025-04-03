// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::histogram::Histogram as MystenHistogram;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use std::sync::Arc;

pub struct CheckpointExecutorMetrics {
    pub checkpoint_exec_sync_tps: IntGauge,
    pub last_executed_checkpoint: IntGauge,
    pub last_executed_checkpoint_timestamp_ms: IntGauge,
    pub checkpoint_exec_errors: IntCounter,
    pub checkpoint_exec_epoch: IntGauge,
    pub checkpoint_exec_inflight: IntGauge,
    pub checkpoint_exec_latency: Histogram,
    pub checkpoint_prepare_latency: Histogram,
    pub checkpoint_transaction_count: Histogram,
    pub checkpoint_contents_age: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    pub checkpoint_contents_age_ms: MystenHistogram,
    pub last_executed_checkpoint_age: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    pub last_executed_checkpoint_age_ms: MystenHistogram,
    pub checkpoint_executor_validator_path: IntGauge,

    pub stage_wait_duration_ns: IntCounterVec,
    pub stage_active_duration_ns: IntCounterVec,
}

impl CheckpointExecutorMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            checkpoint_exec_sync_tps: register_int_gauge_with_registry!(
                "checkpoint_exec_sync_tps",
                "Checkpoint sync estimated transactions per second",
                registry
            )
            .unwrap(),
            last_executed_checkpoint: register_int_gauge_with_registry!(
                "last_executed_checkpoint",
                "Last executed checkpoint",
                registry
            )
            .unwrap(),
            last_executed_checkpoint_timestamp_ms: register_int_gauge_with_registry!(
                "last_executed_checkpoint_timestamp_ms",
                "Last executed checkpoint timestamp ms",
                registry
            )
            .unwrap(),
            checkpoint_exec_errors: register_int_counter_with_registry!(
                "checkpoint_exec_errors",
                "Checkpoint execution errors count",
                registry
            )
            .unwrap(),
            checkpoint_exec_epoch: register_int_gauge_with_registry!(
                "checkpoint_exec_epoch",
                "Current epoch number in the checkpoint executor",
                registry
            )
            .unwrap(),
            checkpoint_exec_inflight: register_int_gauge_with_registry!(
                "checkpoint_exec_inflight",
                "Current number of inflight checkpoints being executed",
                registry
            )
            .unwrap(),
            checkpoint_exec_latency: register_histogram_with_registry!(
                "checkpoint_exec_latency",
                "Latency of executing a checkpoint from enqueue to all effects available",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_prepare_latency: register_histogram_with_registry!(
                "checkpoint_prepare_latency",
                "Latency of preparing a checkpoint to enqueue for execution",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_transaction_count: register_histogram_with_registry!(
                "checkpoint_transaction_count",
                "Number of transactions in the checkpoint",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_contents_age: register_histogram_with_registry!(
                "checkpoint_contents_age",
                "Age of checkpoints when they arrive for execution",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_contents_age_ms: MystenHistogram::new_in_registry(
                "checkpoint_contents_age_ms",
                "Age of checkpoints when they arrive for execution",
                registry,
            ),
            last_executed_checkpoint_age: register_histogram_with_registry!(
                "last_executed_checkpoint_age",
                "Age of the last executed checkpoint",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            last_executed_checkpoint_age_ms: MystenHistogram::new_in_registry(
                "last_executed_checkpoint_age_ms",
                "Age of the last executed checkpoint",
                registry,
            ),
            checkpoint_executor_validator_path: register_int_gauge_with_registry!(
                "checkpoint_executor_validator_path",
                "Number of checkpoints executed using the validator path",
                registry
            )
            .unwrap(),
            stage_wait_duration_ns: register_int_counter_vec_with_registry!(
                "checkpoint_executor_pipeline_stage_wait_duration_ns",
                "Pipeline stage wait duration in nanoseconds",
                &["stage"],
                registry,
            )
            .unwrap(),
            stage_active_duration_ns: register_int_counter_vec_with_registry!(
                "checkpoint_executor_pipeline_stage_active_duration_ns",
                "Pipeline stage active duration in nanoseconds",
                &["stage"],
                registry,
            )
            .unwrap(),
        };
        Arc::new(this)
    }

    pub fn new_for_tests() -> Arc<Self> {
        Self::new(&Registry::new())
    }
}
