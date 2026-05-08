// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::cast_possible_truncation)]
// `ThreadCounters` is written exclusively by its owning thread and read at snapshot points by the
// reporter; we manually `unsafe impl Sync` on it to share `&'static ThreadCounters` via the
// per-context registry without paying for atomics or a Mutex.
#![allow(unsafe_code)]

use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use parking_lot::Mutex;

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

/// Per-thread telemetry counters. Each thread writes only to its own instance, so the writes are
/// single-threaded and need no atomicity. The reporter (`to_runtime_telemetry`) iterates the
/// registry and snapshots each thread's `Cell`s; reads accept that they may observe a write in
/// progress (counters are monotonic and reports are advisory).
#[derive(Debug)]
pub(crate) struct ThreadCounters {
    pub(crate) total_load_time: Cell<u64>,
    pub(crate) load_count: Cell<u64>,
    pub(crate) total_validation_time: Cell<u64>,
    pub(crate) validation_count: Cell<u64>,
    pub(crate) total_jit_time: Cell<u64>,
    pub(crate) jit_count: Cell<u64>,
    pub(crate) total_execution_time: Cell<u64>,
    pub(crate) execution_count: Cell<u64>,
    pub(crate) total_interpreter_time: Cell<u64>,
    pub(crate) interpreter_count: Cell<u64>,
    pub(crate) total_time: Cell<u64>,
    pub(crate) total_count: Cell<u64>,
    pub(crate) redundant_compilations: Cell<u64>,
    pub(crate) max_callstack_size: Cell<u64>,
    pub(crate) max_valuestack_size: Cell<u64>,
}

// SAFETY: `ThreadCounters` is registered into the per-`TelemetryContext` registry the first time
// its owning thread touches telemetry, and from then on only that thread mutates it. The reporter
// reads via `Cell::get` from another thread; any tearing this exposes is acceptable for advisory
// telemetry counters.
unsafe impl Sync for ThreadCounters {}

impl ThreadCounters {
    const fn new() -> Self {
        Self {
            total_load_time: Cell::new(0),
            load_count: Cell::new(0),
            total_validation_time: Cell::new(0),
            validation_count: Cell::new(0),
            total_jit_time: Cell::new(0),
            jit_count: Cell::new(0),
            total_execution_time: Cell::new(0),
            execution_count: Cell::new(0),
            total_interpreter_time: Cell::new(0),
            interpreter_count: Cell::new(0),
            total_time: Cell::new(0),
            total_count: Cell::new(0),
            redundant_compilations: Cell::new(0),
            max_callstack_size: Cell::new(0),
            max_valuestack_size: Cell::new(0),
        }
    }
}

/// Unique id source so a recycled `TelemetryContext` heap address never aliases a previous one's
/// thread-local cache entry.
static NEXT_TELEMETRY_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Id of the `TelemetryContext` whose counters this thread is currently registered with.
    /// `0` means "unregistered".
    static LOCAL_ID: Cell<u64> = const { Cell::new(0) };
    /// `ThreadCounters` registered for `LOCAL_ID`. Heap-leaked on registration so the `'static`
    /// reference outlives both the thread and the context.
    static LOCAL_REF: Cell<Option<&'static ThreadCounters>> = const { Cell::new(None) };
}

/// Telemetry block held by the runtime for global timing information.
/// A U64 should be able to hold approximately 59_9730_287 _years_ worth of milliseconds, so this
/// should be more than large enough for anything we care about. This also means we cannot overflow
/// this value in a single epoch.
/// [SAFETY]: This is thread safe.
#[derive(Debug)]
pub(crate) struct TelemetryContext {
    id: u64,
    threads: Mutex<Vec<&'static ThreadCounters>>,
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
            id: NEXT_TELEMETRY_ID.fetch_add(1, Ordering::Relaxed),
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Returns this thread's counters for this context, registering on first use (or after a
    /// context switch).
    fn local(&self) -> &'static ThreadCounters {
        match LOCAL_REF.with(|c| c.get()) {
            Some(tc) if LOCAL_ID.with(|c| c.get()) == self.id => tc,
            _ => self.register_local(),
        }
    }

    #[cold]
    fn register_local(&self) -> &'static ThreadCounters {
        let leaked: &'static ThreadCounters = Box::leak(Box::new(ThreadCounters::new()));
        self.threads.lock().push(leaked);
        LOCAL_REF.with(|c| c.set(Some(leaked)));
        LOCAL_ID.with(|c| c.set(self.id));
        leaked
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

    /// Update the telemetry by folding the transaction context into this thread's counters.
    pub(crate) fn record_transaction(&self, transaction: TransactionTelemetryContext) {
        let local = self.local();
        macro_rules! add {
            ($field:ident, $delta:expr) => {{
                local.$field.set(local.$field.get().saturating_add($delta));
            }};
        }
        macro_rules! update_duration_field {
            ($duration:expr, $count:expr, $total_field:ident, $count_field:ident) => {{
                if let Some(time) = $duration {
                    add!($total_field, time.as_millis() as u64);
                    add!($count_field, $count);
                }
            }};
            ($duration:expr, $total_field:ident, $count_field:ident) => {{
                if let Some(time) = $duration {
                    add!($total_field, time.as_millis() as u64);
                    add!($count_field, 1);
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

        local
            .max_callstack_size
            .set(local.max_callstack_size.get().max(max_callstack_size));
        local
            .max_valuestack_size
            .set(local.max_valuestack_size.get().max(max_valuestack_size));

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

        add!(total_time, total_time.as_millis() as u64);
        add!(total_count, 1);
        add!(redundant_compilations, redundant_compilations);
    }

    /// Generate a runtime telemetry report from the telemetry data.
    /// This is a touch expensive and should be done infrequently.
    /// [SAFETY] This may produce a partial result if telemetry udpates happen in the middle of
    /// generating the report. This is a known risk, and deemed better than the alternative of
    /// using locks (wherein an RwLock would be read-acquired for the writes and write-acquired for
    /// the read).
    pub fn to_runtime_telemetry(&self, package_cache: &MoveCache) -> MoveRuntimeTelemetry {
        // Aggregate per-thread counters.
        let mut total_load_time = 0u64;
        let mut load_count = 0u64;
        let mut total_validation_time = 0u64;
        let mut validation_count = 0u64;
        let mut total_jit_time = 0u64;
        let mut jit_count = 0u64;
        let mut total_execution_time = 0u64;
        let mut execution_count = 0u64;
        let mut total_interpreter_time = 0u64;
        let mut interpreter_count = 0u64;
        let mut total_time = 0u64;
        let mut total_count = 0u64;
        let mut max_callstack_size = 0u64;
        let mut max_valuestack_size = 0u64;
        for tc in self.threads.lock().iter() {
            total_load_time = total_load_time.saturating_add(tc.total_load_time.get());
            load_count = load_count.saturating_add(tc.load_count.get());
            total_validation_time =
                total_validation_time.saturating_add(tc.total_validation_time.get());
            validation_count = validation_count.saturating_add(tc.validation_count.get());
            total_jit_time = total_jit_time.saturating_add(tc.total_jit_time.get());
            jit_count = jit_count.saturating_add(tc.jit_count.get());
            total_execution_time =
                total_execution_time.saturating_add(tc.total_execution_time.get());
            execution_count = execution_count.saturating_add(tc.execution_count.get());
            total_interpreter_time =
                total_interpreter_time.saturating_add(tc.total_interpreter_time.get());
            interpreter_count = interpreter_count.saturating_add(tc.interpreter_count.get());
            total_time = total_time.saturating_add(tc.total_time.get());
            total_count = total_count.saturating_add(tc.total_count.get());
            max_callstack_size = max_callstack_size.max(tc.max_callstack_size.get());
            max_valuestack_size = max_valuestack_size.max(tc.max_valuestack_size.get());
        }

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
        self.load_count = self.load_count.saturating_add(load_count);
    }

    pub(crate) fn record_validation_time(&mut self, validation_time: Duration, valid_count: u64) {
        add_or_set!(self, validation_time);
        self.validation_count = self.validation_count.saturating_add(valid_count);
    }

    pub(crate) fn record_jit_time(&mut self, jit_time: Duration, jit_count: u64) {
        add_or_set!(self, jit_time);
        self.jit_count = self.jit_count.saturating_add(jit_count);
    }

    pub(crate) fn record_execution_time(&mut self, execution_time: Duration) {
        add_or_set!(self, execution_time)
    }

    pub(crate) fn record_interpreter_time(&mut self, interpreter_time: Duration) {
        add_or_set!(self, interpreter_time)
    }

    pub(crate) fn record_total_time(&mut self, total_time: Duration) {
        self.total_time = self.total_time.saturating_add(total_time);
    }

    pub(crate) fn record_redundant_compilation(&mut self) {
        self.redundant_compilations = self.redundant_compilations.saturating_add(1);
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
