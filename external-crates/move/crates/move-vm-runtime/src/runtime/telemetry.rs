// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::cast_possible_truncation)]
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use crate::cache::move_cache::MoveCache;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// External / Public Information
// -----------------------------------------------

/// Runtime Telemetry Information for reporting
#[derive(Debug)]
pub struct MoveRuntimeTelemetry {
    // -------------------------------------------
    // Telemetry Derived from Cache over Execution
    /// Total packges in the cache
    pub package_cache_count: u64,
    /// Total size of all packages in the cache
    pub total_arena_size: u64,
    /// Total modules of all packages in the cache
    pub module_count: u64,
    /// Total functions of all packages in the cache
    pub function_count: u64,
    /// Total types of all packages in the cache
    pub type_count: u64,
    /// Total size of all string interner
    pub interner_size: u64,
    /// Total number of VTables in the LRU
    pub vtable_cache_count: u64,
    ///  Total number of VTable cache hits
    pub vtable_cache_hits: u64,
    ///  Total number of VTable cache misses
    pub vtable_cache_misses: u64,

    // -------------------------------------------
    // Telemetry Tracked over Execution
    /// Load Time (ms)
    pub total_load_time: u64,
    /// Load Count -- number of individual packages loaded
    pub load_count: u64,
    /// Validation Time (ms)
    pub total_validation_time: u64,
    /// Validation Count -- number of individual packages validated
    /// Note that some validation time is spent on cross-package validation, which is not reflected
    /// in this count.
    pub validation_count: u64,
    /// JIT Time (ms)
    pub total_jit_time: u64,
    /// JIT Count -- number of individual packages JITted
    pub jit_count: u64,
    /// Code Execution Time (ms)
    pub total_execution_time: u64,
    /// Execution Count -- Number of execution calls
    pub execution_count: u64,
    /// Interpreter Time (ms)
    pub total_interpreter_time: u64,
    /// Interpreter Count -- Number of interpreter calls
    pub interpreter_count: u64,
    /// Max Callstack Size -- the maximum callstack size observed across all transactions
    pub max_callstack_size: u64,
    /// Max Value Stack Size -- the maximum value stack size observed across all transactions
    pub max_valuestack_size: u64,
    /// Total Time (ms)
    pub total_time: u64,
    /// Total Count -- Records all interactions with the runtime, including loading for publish; VM
    /// creation; and invocation.
    pub total_count: u64,
}

// -----------------------------------------------
// Internal Context for Reporting / Storage
// -----------------------------------------------

/// Telemetry block held by the runtime for global timing information
/// A U64 should be able to hold approximately 59_9730_287 _years_ worth of milliseconds, so this
/// should be more than large enough for anything we care about. This also means we cannot overflow
/// this value in a single epoch.
/// [SAFETY]: This is thread safe.
#[derive(Debug)]
pub(crate) struct TelemetryContext {
    /// Load Time (ms)
    pub(crate) total_load_time: AtomicU64,
    /// Load Count -- number of individual packages loaded
    pub(crate) load_count: AtomicU64,
    /// Validation Time (ms)
    pub(crate) total_validation_time: AtomicU64,
    /// Validation Count -- number of individual packages validated
    pub(crate) validation_count: AtomicU64,
    /// JIT Time (ms)
    pub(crate) total_jit_time: AtomicU64,
    /// JIT Count -- number of individual packages JITted
    pub(crate) jit_count: AtomicU64,
    /// Code Execution Time (ms)
    pub(crate) total_execution_time: AtomicU64,
    /// Execution Count -- Number of execution calls
    pub(crate) execution_count: AtomicU64,
    /// Interpreter Time (ms)
    pub(crate) total_interpreter_time: AtomicU64,
    /// Interpreter Count -- Number of interpreter calls
    pub(crate) interpreter_count: AtomicU64,
    /// Total Time (ms)
    pub(crate) total_time: AtomicU64,
    /// Total Transaction Count
    pub(crate) total_count: AtomicU64,
    /// Total number of packages that were compiled, but later thrown away since they were already
    /// cached when we went to insert them into it (i.e., redundant compilations)
    pub(crate) redundant_compilations: AtomicU64,
    /// Max Callstack Size -- the maximum callstack size observed for this transactions
    pub(crate) max_callstack_size: AtomicU64,
    /// Max Value Stack Size -- the maximum value stack size observed for this transaction
    pub(crate) max_valuestack_size: AtomicU64,
}

/// Transaction Telemetry Information
/// This is created per-transaction and rolled up into the Telemetry Context after a transaction
/// executes.
/// [SAFETY]: This is not thread safe.
#[derive(Debug)]
pub(crate) struct TransactionTelemetryContext {
    pub load_count: u64,
    pub load_time: Option<Duration>,
    pub validation_count: u64,
    pub validation_time: Option<Duration>,
    pub jit_count: u64,
    pub jit_time: Option<Duration>,
    pub execution_time: Option<Duration>,
    pub interpreter_time: Option<Duration>,
    pub total_time: Duration,
    pub redundant_compilations: u64,
    pub max_callstack_size: u64,
    pub max_valuestack_size: u64,
    // TODO(vm-rewrite): Add value sizes, type sizes, etc?
}

/// Transaction Telemetry Information
/// This is created per-transaction and rolled up into the Telemetry Context after a transaction
/// executes.
#[derive(Debug)]
pub(crate) struct MoveCacheTelemetry {
    /// Total packages in the cache
    pub package_cache_count: u64,
    /// Total size of all packages in the cache
    pub total_arena_size: u64,
    /// Total modules of all packages in the cache
    pub module_count: u64,
    /// Total functions of all packages in the cache
    pub function_count: u64,
    /// Total types of all packages in the cache
    pub type_count: u64,
    /// Total identifiers interned
    pub interner_size: u64,
    /// Total number of VTables in the VTable cache
    pub vtable_cache_count: u64,
    ///  Total number of VTable cache hits
    pub vtable_cache_hits: u64,
    ///  Total number of VTable cache misses
    pub vtable_cache_misses: u64,
}

/// Timer Kinds
#[derive(Debug)]
pub(crate) enum TimerKind {
    Load,
    Validation,
    JIT,
    Execution,
    Interpreter,
    Total,
}

/// Timer
/// Used for timing various parts of the runtime. Must be reported or will panic on drop.
pub(crate) struct Timer {
    kind: TimerKind,
    count: Option<u64>,
    reported: bool,
    start_time: std::time::Instant,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl TelemetryContext {
    pub(crate) fn new() -> Self {
        Self {
            total_load_time: AtomicU64::new(0),
            load_count: AtomicU64::new(0),
            total_execution_time: AtomicU64::new(0),
            execution_count: AtomicU64::new(0),
            total_interpreter_time: AtomicU64::new(0),
            interpreter_count: AtomicU64::new(0),
            total_validation_time: AtomicU64::new(0),
            validation_count: AtomicU64::new(0),
            total_jit_time: AtomicU64::new(0),
            jit_count: AtomicU64::new(0),
            total_time: AtomicU64::new(0),
            total_count: AtomicU64::new(0),
            redundant_compilations: AtomicU64::new(0),
            max_callstack_size: AtomicU64::new(0),
            max_valuestack_size: AtomicU64::new(0),
        }
    }

    pub(crate) fn with_transaction_telemetry<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut TransactionTelemetryContext) -> R,
    {
        let mut txn_telemetry = TransactionTelemetryContext::new();
        let result = f(&mut txn_telemetry);
        self.record_transaction(txn_telemetry);
        result
    }

    /// Update the telemetry by recording the context. Note that this mutates the context in-place
    /// via atomic update operations.
    pub(crate) fn record_transaction(&self, transaction: TransactionTelemetryContext) {
        macro_rules! update_duration_field {
            ($duration:expr, $count:expr, $total_field:ident, $count_field:ident) => {{
                if let Some(time) = $duration {
                    let millis = time.as_millis() as u64;
                    self.$total_field.fetch_add(millis, Ordering::Release);
                    self.$count_field.fetch_add($count, Ordering::Release);
                }
            }};
            ($duration:expr, $total_field:ident, $count_field:ident) => {{
                if let Some(time) = $duration {
                    let millis = time.as_millis() as u64;
                    self.$total_field.fetch_add(millis, Ordering::Release);
                    self.$count_field.fetch_add(1, Ordering::Release);
                }
            }};
        }

        let TransactionTelemetryContext {
            load_count,
            load_time,
            validation_time,
            validation_count,
            jit_time,
            jit_count,
            execution_time,
            interpreter_time,
            total_time,
            redundant_compilations,
            max_callstack_size,
            max_valuestack_size,
        } = transaction;

        let _ = self
            .max_callstack_size
            .fetch_max(max_callstack_size, Ordering::Release);
        let _ = self
            .max_valuestack_size
            .fetch_max(max_valuestack_size, Ordering::Release);

        update_duration_field!(load_time, load_count, total_load_time, load_count);
        update_duration_field!(
            validation_time,
            validation_count,
            total_validation_time,
            validation_count
        );
        update_duration_field!(jit_time, jit_count, total_jit_time, jit_count);

        update_duration_field!(execution_time, total_execution_time, execution_count);
        update_duration_field!(interpreter_time, total_interpreter_time, interpreter_count);

        let total_millis = total_time.as_millis() as u64;
        self.total_time.fetch_add(total_millis, Ordering::Release);
        self.total_count.fetch_add(1, Ordering::Release);
        self.redundant_compilations
            .fetch_add(redundant_compilations, Ordering::Release);
    }

    /// Generate a runtime telemetry report from the telemetry data.
    /// This is a touch expensive and should be done infrequently.
    /// [SAFETY] This may produce a partial result if telemetry udpates happen in the middle of
    /// generating the report. This is a known risk, and deemed better than the alternative of
    /// using locks (wherein an RwLock would be read-acquired for the writes and write-acquired for
    /// the read).
    pub fn to_runtime_telemetry(&self, package_cache: &MoveCache) -> MoveRuntimeTelemetry {
        // Read atomic telemetry values.
        let total_load_time = self.total_load_time.load(Ordering::Relaxed);
        let load_count = self.load_count.load(Ordering::Relaxed);
        let total_validation_time = self.total_validation_time.load(Ordering::Relaxed);
        let validation_count = self.validation_count.load(Ordering::Relaxed);
        let total_jit_time = self.total_jit_time.load(Ordering::Relaxed);
        let jit_count = self.jit_count.load(Ordering::Relaxed);
        let total_execution_time = self.total_execution_time.load(Ordering::Relaxed);
        let execution_count = self.execution_count.load(Ordering::Relaxed);
        let total_interpreter_time = self.total_interpreter_time.load(Ordering::Relaxed);
        let interpreter_count = self.interpreter_count.load(Ordering::Relaxed);
        let max_callstack_size = self.max_callstack_size.load(Ordering::Relaxed);
        let max_valuestack_size = self.max_valuestack_size.load(Ordering::Relaxed);
        let total_time = self.total_time.load(Ordering::Relaxed);
        let total_count = self.total_count.load(Ordering::Relaxed);

        // Retrieve package cache statistics.
        let MoveCacheTelemetry {
            package_cache_count,
            total_arena_size,
            module_count,
            function_count,
            type_count,
            interner_size,
            vtable_cache_count,
            vtable_cache_hits,
            vtable_cache_misses,
        } = package_cache.to_cache_telemetry();

        MoveRuntimeTelemetry {
            // Cache information.
            package_cache_count,
            total_arena_size,
            module_count,
            function_count,
            type_count,
            interner_size,
            vtable_cache_count,
            vtable_cache_hits,
            vtable_cache_misses,

            // Telemetry metrics.
            total_load_time,
            load_count,
            total_validation_time,
            validation_count,
            total_jit_time,
            jit_count,
            total_execution_time,
            execution_count,
            total_interpreter_time,
            interpreter_count,
            max_callstack_size,
            max_valuestack_size,
            total_time,
            total_count,
        }
    }
}

macro_rules! add_or_set {
    ($self:ident, $name:ident) => {{
        $self.$name = $self.$name.map(|cur| cur + $name).or_else(|| Some($name));
    }};
}

impl TransactionTelemetryContext {
    pub(crate) fn new() -> Self {
        Self {
            load_count: 0,
            load_time: None,
            validation_count: 0,
            validation_time: None,
            jit_count: 0,
            jit_time: None,
            execution_time: None,
            interpreter_time: None,
            total_time: Duration::new(0, 0),
            redundant_compilations: 0,
            max_callstack_size: 0,
            max_valuestack_size: 0,
        }
    }

    pub(crate) fn make_timer(&self, kind: TimerKind) -> Timer {
        Timer {
            kind,
            count: None,
            reported: false,
            start_time: std::time::Instant::now(),
        }
    }

    pub(crate) fn make_timer_with_count(&self, kind: TimerKind, count: u64) -> Timer {
        Timer {
            kind,
            count: Some(count),
            reported: false,
            start_time: std::time::Instant::now(),
        }
    }

    pub(crate) fn report_time(&mut self, timer: Timer) {
        let duration = timer.start_time.elapsed();
        let mut timer = timer;
        timer.reported = true;
        match timer.kind {
            TimerKind::Load => {
                debug_assert!(
                    timer.count.is_some(),
                    "Load timer must have a count associated with it"
                );
                let Some(count) = timer.count else { return };
                self.record_load_time(duration, count);
            }
            TimerKind::Validation => {
                debug_assert!(
                    timer.count.is_some(),
                    "Validation timer must have a count associated with it"
                );
                let Some(count) = timer.count else { return };
                self.record_validation_time(duration, count);
            }
            TimerKind::JIT => {
                debug_assert!(
                    timer.count.is_some(),
                    "JIT timer must have a count associated with it"
                );
                let Some(count) = timer.count else { return };
                self.record_jit_time(duration, count);
            }
            TimerKind::Execution => {
                self.record_execution_time(duration);
            }
            TimerKind::Interpreter => {
                self.record_interpreter_time(duration);
            }
            TimerKind::Total => {
                self.record_total_time(duration);
            }
        }
    }

    pub(crate) fn record_load_time(&mut self, load_time: Duration, load_count: u64) {
        add_or_set!(self, load_time);
        self.load_count += load_count;
    }

    pub(crate) fn record_validation_time(&mut self, validation_time: Duration, valid_count: u64) {
        add_or_set!(self, validation_time);
        self.validation_count += valid_count;
    }

    pub(crate) fn record_jit_time(&mut self, jit_time: Duration, jit_count: u64) {
        add_or_set!(self, jit_time);
        self.jit_count += jit_count;
    }

    pub(crate) fn record_execution_time(&mut self, execution_time: Duration) {
        add_or_set!(self, execution_time)
    }

    pub(crate) fn record_interpreter_time(&mut self, interpreter_time: Duration) {
        add_or_set!(self, interpreter_time)
    }

    pub(crate) fn record_total_time(&mut self, total_time: Duration) {
        self.total_time += total_time;
    }

    pub(crate) fn record_redundant_compilation(&mut self) {
        self.redundant_compilations += 1;
    }

    pub(crate) fn record_callstack_size(&mut self, callstack_size: u64) {
        self.max_callstack_size = self.max_callstack_size.max(callstack_size);
    }

    pub(crate) fn record_valuestack_size(&mut self, valuestack_size: u64) {
        self.max_valuestack_size = self.max_valuestack_size.max(valuestack_size);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if !self.reported {
            debug_assert!(
                false,
                "Timer of kind {:?} was not recorded before drop",
                self.kind
            );
        }
    }
}
