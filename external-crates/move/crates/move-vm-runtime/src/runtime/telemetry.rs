// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::cast_possible_truncation)]

use std::{
    cell::RefCell,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
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

/// Per-thread telemetry counters. Each thread is the only writer to its own instance, so
/// `fetch_add(_, Relaxed)` runs on a cache line owned exclusively by that thread's core (no
/// cross-core MESI traffic). The reporter reads with `load(Relaxed)` and accepts that it may
/// observe a write in progress; counters are monotonic and reports are advisory.
#[derive(Debug, Default)]
pub(crate) struct ThreadCounters {
    pub(crate) total_load_time: AtomicU64,
    pub(crate) load_count: AtomicU64,
    pub(crate) total_validation_time: AtomicU64,
    pub(crate) validation_count: AtomicU64,
    pub(crate) total_jit_time: AtomicU64,
    pub(crate) jit_count: AtomicU64,
    pub(crate) total_execution_time: AtomicU64,
    pub(crate) execution_count: AtomicU64,
    pub(crate) total_interpreter_time: AtomicU64,
    pub(crate) interpreter_count: AtomicU64,
    pub(crate) total_time: AtomicU64,
    pub(crate) total_count: AtomicU64,
    pub(crate) redundant_compilations: AtomicU64,
    pub(crate) max_callstack_size: AtomicU64,
    pub(crate) max_valuestack_size: AtomicU64,
}

/// Unique id source so a recycled `TelemetryContext` heap address never aliases a previous one's
/// thread-local cache entry.
static NEXT_TELEMETRY_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// `(context_id, slot)` for the context this thread is currently registered with. Replacing
    /// or dropping this folds the slot's counters into the context's aggregated total via
    /// `LocalSlot::drop`.
    static LOCAL: RefCell<Option<(u64, LocalSlot)>> = const { RefCell::new(None) };
}

/// Plain-`u64` snapshot used both for the dead-thread aggregate and the final report. Kept
/// separate from `ThreadCounters` so reads through this type don't pay any atomic cost.
#[derive(Debug, Default, Clone)]
struct CountersSnapshot {
    total_load_time: u64,
    load_count: u64,
    total_validation_time: u64,
    validation_count: u64,
    total_jit_time: u64,
    jit_count: u64,
    total_execution_time: u64,
    execution_count: u64,
    total_interpreter_time: u64,
    interpreter_count: u64,
    total_time: u64,
    total_count: u64,
    redundant_compilations: u64,
    max_callstack_size: u64,
    max_valuestack_size: u64,
}

impl CountersSnapshot {
    fn add_from(&mut self, c: &ThreadCounters) {
        self.total_load_time = self
            .total_load_time
            .saturating_add(c.total_load_time.load(Ordering::Relaxed));
        self.load_count = self
            .load_count
            .saturating_add(c.load_count.load(Ordering::Relaxed));
        self.total_validation_time = self
            .total_validation_time
            .saturating_add(c.total_validation_time.load(Ordering::Relaxed));
        self.validation_count = self
            .validation_count
            .saturating_add(c.validation_count.load(Ordering::Relaxed));
        self.total_jit_time = self
            .total_jit_time
            .saturating_add(c.total_jit_time.load(Ordering::Relaxed));
        self.jit_count = self
            .jit_count
            .saturating_add(c.jit_count.load(Ordering::Relaxed));
        self.total_execution_time = self
            .total_execution_time
            .saturating_add(c.total_execution_time.load(Ordering::Relaxed));
        self.execution_count = self
            .execution_count
            .saturating_add(c.execution_count.load(Ordering::Relaxed));
        self.total_interpreter_time = self
            .total_interpreter_time
            .saturating_add(c.total_interpreter_time.load(Ordering::Relaxed));
        self.interpreter_count = self
            .interpreter_count
            .saturating_add(c.interpreter_count.load(Ordering::Relaxed));
        self.total_time = self
            .total_time
            .saturating_add(c.total_time.load(Ordering::Relaxed));
        self.total_count = self
            .total_count
            .saturating_add(c.total_count.load(Ordering::Relaxed));
        self.redundant_compilations = self
            .redundant_compilations
            .saturating_add(c.redundant_compilations.load(Ordering::Relaxed));
        self.max_callstack_size = self
            .max_callstack_size
            .max(c.max_callstack_size.load(Ordering::Relaxed));
        self.max_valuestack_size = self
            .max_valuestack_size
            .max(c.max_valuestack_size.load(Ordering::Relaxed));
    }
}

/// TLS-owned guard. Holds the only strong `Arc` to this thread's counters; on drop (thread exit
/// or context switch) it folds counters into the context's aggregated total before releasing the
/// `Arc`. After the `Arc` drops, the registry's `Weak` will upgrade to `None` and the reporter
/// removes the entry on its next sweep.
struct LocalSlot {
    counters: Arc<ThreadCounters>,
    ctx: Weak<TelemetryContext>,
}

impl Drop for LocalSlot {
    fn drop(&mut self) {
        if let Some(ctx) = self.ctx.upgrade() {
            // Hold `threads` across the Arc drop so the reporter cannot observe `aggregated`
            // already updated while our `Weak` is still upgradeable. Lock order matches
            // `to_runtime_telemetry`: threads, then aggregated.
            let _threads = ctx.threads.lock();
            // Declaration order is load-bearing: `_counters` drops before `_threads`, so the
            // strong Arc dies while we still hold `threads`. The reporter's next sweep then
            // sees our `Weak` fail to upgrade and evicts it.
            let _counters = std::mem::take(&mut self.counters);
            ctx.aggregated.lock().add_from(&_counters);
        }
    }
}

/// Telemetry block held by the runtime for global timing information.
/// A U64 should be able to hold approximately 59_9730_287 _years_ worth of milliseconds, so this
/// should be more than large enough for anything we care about. This also means we cannot overflow
/// this value in a single epoch.
#[derive(Debug)]
pub(crate) struct TelemetryContext {
    id: u64,
    /// Counters folded in from threads/contexts that have already torn down. Locked briefly by
    /// dying-thread `Drop` and by the reporter; never on the per-transaction path.
    aggregated: Mutex<CountersSnapshot>,
    /// Live thread counters. `Weak` so the registry never extends `ThreadCounters'` lifetime —
    /// the strong `Arc` lives only in TLS.
    threads: Mutex<Vec<Weak<ThreadCounters>>>,
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
            aggregated: Mutex::new(CountersSnapshot::default()),
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Run `f` against this thread's counters for this context, registering on first use (or
    /// after a context switch). The closure form keeps the strong `Arc` inside TLS — no Arc
    /// clone, no raw pointer cache, no `unsafe`.
    fn with_local<R>(self: &Arc<Self>, f: impl FnOnce(&ThreadCounters) -> R) -> R {
        LOCAL.with_borrow_mut(|slot| {
            let local: &mut LocalSlot = match slot {
                Some((id, local)) if *id == self.id => local,
                // Replacing the slot via `Option::insert` drops the old `LocalSlot`; its `Drop`
                // folds the previous context's counters into that context's aggregated total.
                other => &mut other.insert((self.id, self.register_local())).1,
            };
            f(&local.counters)
        })
    }

    #[cold]
    fn register_local(self: &Arc<Self>) -> LocalSlot {
        let counters = Arc::new(ThreadCounters::default());
        self.threads.lock().push(Arc::downgrade(&counters));
        LocalSlot {
            counters,
            ctx: Arc::downgrade(self),
        }
    }

    pub(crate) fn with_transaction_telemetry<F, R>(self: &Arc<Self>, f: F) -> R
    where
        F: FnOnce(&mut TransactionTelemetryContext) -> R,
    {
        let mut txn_telemetry = TransactionTelemetryContext::new();
        let result = f(&mut txn_telemetry);
        self.record_transaction(txn_telemetry);
        result
    }

    /// Update the telemetry by folding the transaction context into this thread's counters.
    pub(crate) fn record_transaction(self: &Arc<Self>, transaction: TransactionTelemetryContext) {
        self.with_local(|local| {
            macro_rules! add {
                ($field:ident, $delta:expr) => {{
                    local.$field.fetch_add($delta, Ordering::Relaxed);
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
                .fetch_max(max_callstack_size, Ordering::Relaxed);
            local
                .max_valuestack_size
                .fetch_max(max_valuestack_size, Ordering::Relaxed);

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
        });
    }

    /// Generate a runtime telemetry report from the telemetry data.
    /// This is a touch expensive and should be done infrequently.
    /// May produce a partial result if telemetry updates happen mid-report; this is a known and
    /// accepted risk for advisory counters.
    pub fn to_runtime_telemetry(
        self: &Arc<Self>,
        package_cache: &MoveCache,
    ) -> MoveRuntimeTelemetry {
        // Single sweep: lock both `threads` and `aggregated`. `LocalSlot::drop` acquires the
        // same pair in the same order, so the reporter and any concurrent teardown are mutually
        // exclusive — each thread's counters are observed either via its still-upgradeable
        // `Weak` (and not yet in `aggregated`) or via `aggregated` (with its `Weak` already
        // invalidated), never both.
        let totals = {
            let mut threads = self.threads.lock();
            let agg = self.aggregated.lock();
            let mut totals = agg.clone();
            threads.retain(|w| match w.upgrade() {
                Some(arc) => {
                    totals.add_from(&arc);
                    true
                }
                // `LocalSlot::drop` already folded into `aggregated`.
                None => false,
            });
            totals
        };

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
            total_load_time: totals.total_load_time,
            load_count: totals.load_count,
            total_validation_time: totals.total_validation_time,
            validation_count: totals.validation_count,
            total_jit_time: totals.total_jit_time,
            jit_count: totals.jit_count,
            total_execution_time: totals.total_execution_time,
            execution_count: totals.execution_count,
            total_interpreter_time: totals.total_interpreter_time,
            interpreter_count: totals.interpreter_count,
            max_callstack_size: totals.max_callstack_size,
            max_valuestack_size: totals.max_valuestack_size,
            total_time: totals.total_time,
            total_count: totals.total_count,
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
