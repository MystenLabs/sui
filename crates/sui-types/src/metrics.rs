// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry, Histogram,
    IntCounterVec,
};

pub struct LimitsMetrics {
    /// Execution limits metrics
    pub excessive_estimated_effects_size: IntCounterVec,
    pub excessive_written_objects_size: IntCounterVec,
    pub excessive_new_move_object_ids: IntCounterVec,
    pub excessive_deleted_move_object_ids: IntCounterVec,
    pub excessive_transferred_move_object_ids: IntCounterVec,
    pub excessive_object_runtime_cached_objects: IntCounterVec,
    pub excessive_object_runtime_store_entries: IntCounterVec,
}

impl LimitsMetrics {
    pub fn new(registry: &prometheus::Registry) -> LimitsMetrics {
        Self {
            excessive_estimated_effects_size: register_int_counter_vec_with_registry!(
                "excessive_estimated_effects_size",
                "Number of transactions with estimated effects size exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_written_objects_size: register_int_counter_vec_with_registry!(
                "excessive_written_objects_size",
                "Number of transactions with written objects size exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_new_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_new_move_object_ids_size",
                "Number of transactions with new move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_deleted_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_deleted_move_object_ids_size",
                "Number of transactions with deleted move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_transferred_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_transferred_move_object_ids_size",
                "Number of transactions with transferred move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_object_runtime_cached_objects: register_int_counter_vec_with_registry!(
                "excessive_object_runtime_cached_objects_size",
                "Number of transactions with object runtime cached object count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_object_runtime_store_entries: register_int_counter_vec_with_registry!(
                "excessive_object_runtime_store_entries_size",
                "Number of transactions with object runtime store entry count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
        }
    }
}

pub struct BytecodeVerifierMetrics {
    /// Bytecode verifier metrics timeout counter
    pub verifier_timeout_metrics: IntCounterVec,
    /// Bytecode verifier runtime latency for each module successfully verified
    pub verifier_runtime_per_module_success_latency: Histogram,
    /// Bytecode verifier runtime latency for each programmable transaction block successfully verified
    pub verifier_runtime_per_ptb_success_latency: Histogram,
    /// Bytecode verifier runtime latency for each module which timed out
    pub verifier_runtime_per_module_timeout_latency: Histogram,
    /// Bytecode verifier runtime latency for each programmable transaction block which timed out
    pub verifier_runtime_per_ptb_timeout_latency: Histogram,
}

impl BytecodeVerifierMetrics {
    /// DEPRECATED in latest metered verifier, which only report overall success or timeout.
    pub const MOVE_VERIFIER_TAG: &'static str = "move_verifier";

    /// DEPRECATED in latest metered verifier, which only report overall success or timeout.
    pub const SUI_VERIFIER_TAG: &'static str = "sui_verifier";

    pub const OVERALL_TAG: &'static str = "overall";
    pub const SUCCESS_TAG: &'static str = "success";
    pub const TIMEOUT_TAG: &'static str = "failed";
    const LATENCY_SEC_BUCKETS: &'static [f64] = &[
        0.000_010, 0.000_025, 0.000_050, 0.000_100, /* sub 100 micros */
        0.000_250, 0.000_500, 0.001_000, 0.002_500, 0.005_000, 0.010_000, /* sub 10 ms: p99 */
        0.025_000, 0.050_000, 0.100_000, 0.250_000, 0.500_000, 1.000_000, /* sub 1 s */
        10.000_000, 20.000_000, 50.000_000, 100.0, /* We should almost never get here */
    ];
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            verifier_timeout_metrics: register_int_counter_vec_with_registry!(
                "verifier_timeout_metrics",
                "Number of timeouts in bytecode verifier",
                &["verifier_meter", "status"],
                registry,
            )
            .unwrap(),
            verifier_runtime_per_module_success_latency: register_histogram_with_registry!(
                "verifier_runtime_per_module_success_latency",
                "Time spent running bytecode verifier to completion at `run_metered_move_bytecode_verifier_impl`",
                Self::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            verifier_runtime_per_ptb_success_latency: register_histogram_with_registry!(
                "verifier_runtime_per_ptb_success_latency",
                "Time spent running bytecode verifier to completion over the entire PTB at `transaction_input_checker::check_non_system_packages_to_be_published`",
                Self::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            verifier_runtime_per_module_timeout_latency:  register_histogram_with_registry!(
                "verifier_runtime_per_module_timeout_latency",
                "Time spent running bytecode verifier to timeout at `run_metered_move_bytecode_verifier_impl`",
                Self::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            verifier_runtime_per_ptb_timeout_latency: register_histogram_with_registry!(
                "verifier_runtime_per_ptb_timeout_latency",
                "Time spent running bytecode verifier to timeout over the entire PTB at `transaction_input_checker::check_non_system_packages_to_be_published`",
                Self::LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
        }
    }
}
