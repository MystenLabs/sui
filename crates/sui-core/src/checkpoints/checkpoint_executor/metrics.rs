// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::histogram::Histogram;
use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use std::sync::Arc;

pub struct CheckpointExecutorMetrics {
    pub checkpoint_exec_sync_tps: IntGauge,
    pub last_executed_checkpoint: IntGauge,
    pub checkpoint_exec_errors: IntCounter,
    pub checkpoint_exec_epoch: IntGauge,
    pub checkpoint_exec_inflight: IntGauge,
    pub checkpoint_exec_latency_us: Histogram,
    pub checkpoint_prepare_latency_us: Histogram,
    pub checkpoint_transaction_count: Histogram,
    pub checkpoint_contents_age_ms: Histogram,
    pub accumulator_inconsistent_state: IntGauge,
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
            checkpoint_exec_latency_us: Histogram::new_in_registry(
                "checkpoint_exec_latency_us",
                "Latency of executing a checkpoint from enqueue to all effects available, in microseconds",
                registry,
            ),
            checkpoint_prepare_latency_us: Histogram::new_in_registry(
                "checkpoint_prepare_latency_us",
                "Latency of preparing a checkpoint to enqueue for execution, in microseconds",
                registry,
            ),
            checkpoint_transaction_count: Histogram::new_in_registry(
                "checkpoint_transaction_count",
                "Number of transactions in the checkpoint",
                registry,
            ),
            checkpoint_contents_age_ms: Histogram::new_in_registry(
                "checkpoint_contents_age_ms",
                "Age of checkpoints when they arrive for execution",
                registry,
            ),
            accumulator_inconsistent_state: register_int_gauge_with_registry!(
                "accumulator_inconsistent_state",
                "1 if accumulated live object set differs from StateAccumulator root state hash for the previous epoch",
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
