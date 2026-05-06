// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use prometheus::{
    Histogram, IntCounterVec, IntGauge, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_with_registry,
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

/// Combined execution metrics passed into executor methods.
pub struct ExecutionMetrics {
    pub limits_metrics: LimitsMetrics,
    pub vm_telemetry_metrics: MoveVMTelemetryMetrics,
}

impl ExecutionMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            limits_metrics: LimitsMetrics::new(registry),
            vm_telemetry_metrics: MoveVMTelemetryMetrics::new(registry),
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

/// Prometheus metrics for Move VM runtime telemetry, updated periodically via
/// time-based sampling.
pub struct MoveVMTelemetryMetrics {
    pub move_vm_package_cache_count: IntGauge,
    pub move_vm_total_arena_size_bytes: IntGauge,
    pub move_vm_module_count: IntGauge,
    pub move_vm_function_count: IntGauge,
    pub move_vm_type_count: IntGauge,
    pub move_vm_interner_size: IntGauge,
    pub move_vm_vtable_cache_count: IntGauge,
    pub move_vm_vtable_cache_hits: IntGauge,
    pub move_vm_vtable_cache_misses: IntGauge,
    pub move_vm_load_time_ms: IntGauge,
    pub move_vm_load_count: IntGauge,
    pub move_vm_validation_time_ms: IntGauge,
    pub move_vm_validation_count: IntGauge,
    pub move_vm_jit_time_ms: IntGauge,
    pub move_vm_jit_count: IntGauge,
    pub move_vm_execution_time_ms: IntGauge,
    pub move_vm_execution_count: IntGauge,
    pub move_vm_interpreter_time_ms: IntGauge,
    pub move_vm_interpreter_count: IntGauge,
    pub move_vm_max_callstack_size: IntGauge,
    pub move_vm_max_valuestack_size: IntGauge,
    pub move_vm_total_time_ms: IntGauge,
    pub move_vm_total_count: IntGauge,
    last_report_ms: AtomicU64,
}

impl MoveVMTelemetryMetrics {
    const REPORT_INTERVAL_MS: u64 = 30_000;

    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            move_vm_package_cache_count: register_int_gauge_with_registry!(
                "move_vm_package_cache_count",
                "Number of packages in the Move VM cache",
                registry,
            )
            .unwrap(),
            move_vm_total_arena_size_bytes: register_int_gauge_with_registry!(
                "move_vm_total_arena_size_bytes",
                "Total arena memory of cached Move VM packages in bytes",
                registry,
            )
            .unwrap(),
            move_vm_module_count: register_int_gauge_with_registry!(
                "move_vm_module_count",
                "Total modules across cached Move VM packages",
                registry,
            )
            .unwrap(),
            move_vm_function_count: register_int_gauge_with_registry!(
                "move_vm_function_count",
                "Total functions across cached Move VM packages",
                registry,
            )
            .unwrap(),
            move_vm_type_count: register_int_gauge_with_registry!(
                "move_vm_type_count",
                "Total types across cached Move VM packages",
                registry,
            )
            .unwrap(),
            move_vm_interner_size: register_int_gauge_with_registry!(
                "move_vm_interner_size",
                "Number of entries in the Move VM string interner",
                registry,
            )
            .unwrap(),
            move_vm_vtable_cache_count: register_int_gauge_with_registry!(
                "move_vm_vtable_cache_count",
                "Number of entries in the Move VM VTable cache",
                registry,
            )
            .unwrap(),
            move_vm_vtable_cache_hits: register_int_gauge_with_registry!(
                "move_vm_vtable_cache_hits",
                "Cumulative VTable cache hits in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_vtable_cache_misses: register_int_gauge_with_registry!(
                "move_vm_vtable_cache_misses",
                "Cumulative VTable cache misses in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_load_time_ms: register_int_gauge_with_registry!(
                "move_vm_load_time_ms",
                "Cumulative package load time in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_load_count: register_int_gauge_with_registry!(
                "move_vm_load_count",
                "Cumulative number of packages loaded by the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_validation_time_ms: register_int_gauge_with_registry!(
                "move_vm_validation_time_ms",
                "Cumulative validation time in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_validation_count: register_int_gauge_with_registry!(
                "move_vm_validation_count",
                "Cumulative number of validations in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_jit_time_ms: register_int_gauge_with_registry!(
                "move_vm_jit_time_ms",
                "Cumulative JIT compilation time in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_jit_count: register_int_gauge_with_registry!(
                "move_vm_jit_count",
                "Cumulative number of JIT compilations in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_execution_time_ms: register_int_gauge_with_registry!(
                "move_vm_execution_time_ms",
                "Cumulative execution time in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_execution_count: register_int_gauge_with_registry!(
                "move_vm_execution_count",
                "Cumulative number of execution calls in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_interpreter_time_ms: register_int_gauge_with_registry!(
                "move_vm_interpreter_time_ms",
                "Cumulative interpreter time in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_interpreter_count: register_int_gauge_with_registry!(
                "move_vm_interpreter_count",
                "Cumulative number of interpreter calls in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_max_callstack_size: register_int_gauge_with_registry!(
                "move_vm_max_callstack_size",
                "Maximum observed callstack depth in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_max_valuestack_size: register_int_gauge_with_registry!(
                "move_vm_max_valuestack_size",
                "Maximum observed value stack size in the Move VM",
                registry,
            )
            .unwrap(),
            move_vm_total_time_ms: register_int_gauge_with_registry!(
                "move_vm_total_time_ms",
                "Cumulative total time spent in the Move VM (ms)",
                registry,
            )
            .unwrap(),
            move_vm_total_count: register_int_gauge_with_registry!(
                "move_vm_total_count",
                "Cumulative total number of Move VM interactions",
                registry,
            )
            .unwrap(),
            last_report_ms: AtomicU64::new(0),
        }
    }

    /// Update gauges if the reporting interval has elapsed. The closure is only called
    /// when an update is due, avoiding the expensive `get_telemetry_report()` cache scan
    /// on every transaction. The closure receives `&Self` so it can set gauges directly.
    pub fn try_update(&self, f: impl FnOnce(&Self)) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as u64;
        let last = self.last_report_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) < Self::REPORT_INTERVAL_MS {
            return;
        }
        self.last_report_ms.store(now_ms, Ordering::Relaxed);
        f(self);
    }
}
