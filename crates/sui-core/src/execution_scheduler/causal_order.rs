// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Causal-order bookkeeping for execution scheduling.
//!
//! Every unit the ExecutionScheduler enqueues (a transaction, or a key that later
//! materializes into transactions) is assigned a *causal index*: a position in the
//! order in which units were enqueued from consensus handler / checkpoint executor.
//! Enqueue order is causal: anything a transaction can wait for during execution is
//! produced by a unit with a lower index. The execution driver uses these indices to
//! admit transactions in a way that makes blocking on not-yet-available values
//! deadlock-free (see the design comment in `execution_driver.rs`).
//!
//! Index lifecycle. [`CausalWindow::assign`] hands out the next index wrapped in a
//! [`CausalIndexGuard`], which travels with the unit inside its `ExecutionEnv`.
//! Cloning the env (and with it the guard) shares the index among all transactions
//! materialized from one key (a group). The index is *done* when the last guard clone
//! drops - whether because every transaction of the group finished executing (the env
//! is consumed at the end of execution), or because the unit was dropped without
//! executing (already executed, wrong epoch, epoch ended, scheduling skipped). This
//! makes retirement structural: there is no code path that can leak an index without
//! leaking the guard itself, and a leaked index would permanently stall the watermark.
//!
//! One rule must hold for assignment: a unit that other, already-indexed units may wait
//! on must never be re-assigned a *new, higher* index (its waiters would be parked
//! below it, and the admission window never reaches far above a parked index). Units
//! that will execute later keep their original guard - a retry path clones the env
//! before execution finishes; only units that are truly done may drop it.

use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::Notify;

/// Shared state between the ExecutionScheduler (index assignment) and the execution
/// driver (admission). See the module comment and `execution_driver.rs`.
pub struct CausalWindow {
    inner: Mutex<WindowInner>,
    /// Wakes the driver loop when the watermark or in-flight count changes.
    notify: Notify,
    /// The next index to assign. Indices start at 1; watermark 0 means "nothing done".
    next_index: AtomicU64,
    /// Max transactions admitted for execution concurrently via the window branch (K).
    concurrency_limit: usize,
    /// Max distance an admitted index may run ahead of the watermark (W).
    window_size: u64,
}

struct WindowInner {
    /// The watermark C: every index <= C is done (executed or retired).
    watermark: u64,
    /// Done indices > C, awaiting the gap below them to fill.
    done_above: BTreeSet<u64>,
    /// Admitted (submitted for execution) transactions not yet finished. Includes
    /// transactions blocked inside execution.
    in_flight: usize,
}

impl CausalWindow {
    /// Default sizing: execution parallelism K at half the CPUs (leaving the rest for
    /// consensus, networking and checkpointing), window W at 4K so run-ahead past a
    /// slow-to-materialize unit (e.g. a settlement awaiting its batch) is not throttled
    /// by K. Under msim both are fixed: host CPU count must not influence simulation
    /// behavior, and the sim's blocking pool (default 32 threads, shared with other
    /// spawn_blocking users) must accommodate K plus slack.
    pub fn new_with_default_sizing() -> Arc<Self> {
        #[cfg(msim)]
        return Self::new(4, 16);
        #[cfg(not(msim))]
        {
            let concurrency_limit = std::cmp::max(1, num_cpus::get() / 2);
            Self::new(concurrency_limit, 4 * concurrency_limit as u64)
        }
    }

    pub fn new(concurrency_limit: usize, window_size: u64) -> Arc<Self> {
        assert!(concurrency_limit > 0);
        // W must exceed 1 or the second admission branch is empty and execution
        // degenerates to fully serial.
        assert!(window_size > 1);
        Arc::new(Self {
            inner: Mutex::new(WindowInner {
                watermark: 0,
                done_above: BTreeSet::new(),
                in_flight: 0,
            }),
            notify: Notify::new(),
            next_index: AtomicU64::new(1),
            concurrency_limit,
            window_size,
        })
    }

    /// The number of pool threads needed so that admission never has to queue:
    /// concurrency_limit admissions via the window branch, plus the always-admitted
    /// next-consecutive transaction, plus slack for multiple concurrent members of one
    /// group (settlement batches).
    pub fn required_pool_size(&self) -> usize {
        self.window_size as usize + 1
    }

    /// Assigns the next causal index. Callers must call this in causal order (the
    /// order units are enqueued from consensus handler / checkpoint executor).
    pub fn assign(self: &Arc<Self>) -> CausalIndexGuard {
        let index = self.next_index.fetch_add(1, Ordering::Relaxed);
        CausalIndexGuard(Arc::new(GuardInner {
            index,
            window: self.clone(),
        }))
    }

    /// Attempts to admit the transaction with `index` for execution. On success,
    /// returns a slot that must be held for the duration of execution; dropping it
    /// releases the concurrency slot.
    ///
    /// Admission policy: the next transaction in causal order is always admitted (it
    /// can never block, and a pool thread is always available for it - this is what
    /// guarantees progress). Anything else is admitted only within the window and
    /// under the concurrency limit.
    pub fn try_admit(self: &Arc<Self>, index: u64) -> Option<InFlightSlot> {
        let mut inner = self.inner.lock().unwrap();
        debug_assert!(index > inner.watermark, "admitting an already-done index");
        let admit = index == inner.watermark + 1
            || (index < inner.watermark + self.window_size
                && inner.in_flight < self.concurrency_limit);
        if !admit {
            return None;
        }
        inner.in_flight += 1;
        Some(InFlightSlot {
            window: self.clone(),
        })
    }

    /// Waits until the watermark or in-flight count may have changed. A single permit
    /// is buffered, so a change occurring before this call is not missed.
    pub async fn changed(&self) {
        self.notify.notified().await;
    }

    fn mark_done(&self, index: u64) {
        let mut inner = self.inner.lock().unwrap();
        if index == inner.watermark + 1 {
            inner.watermark = index;
            loop {
                let next = inner.watermark + 1;
                if !inner.done_above.remove(&next) {
                    break;
                }
                inner.watermark = next;
            }
        } else {
            debug_assert!(index > inner.watermark, "index done twice");
            inner.done_above.insert(index);
        }
        drop(inner);
        self.notify.notify_one();
    }

    #[cfg(test)]
    pub fn watermark_for_testing(&self) -> u64 {
        self.inner.lock().unwrap().watermark
    }
}

struct GuardInner {
    index: u64,
    window: Arc<CausalWindow>,
}

impl Drop for GuardInner {
    fn drop(&mut self) {
        self.window.mark_done(self.index);
    }
}

/// Owns (a share of) a causal index. The index is marked done when the last clone
/// drops. See the module comment for the lifecycle.
#[derive(Clone)]
pub struct CausalIndexGuard(Arc<GuardInner>);

impl CausalIndexGuard {
    pub fn index(&self) -> u64 {
        self.0.index
    }
}

impl std::fmt::Debug for CausalIndexGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CausalIndexGuard")
            .field(&self.0.index)
            .finish()
    }
}

/// A held execution-concurrency slot; dropping it releases the slot and wakes the
/// driver.
pub struct InFlightSlot {
    window: Arc<CausalWindow>,
}

impl Drop for InFlightSlot {
    fn drop(&mut self) {
        let mut inner = self.window.inner.lock().unwrap();
        inner.in_flight -= 1;
        drop(inner);
        self.window.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watermark_advances_over_done_indices() {
        let window = CausalWindow::new(2, 8);
        let g1 = window.assign();
        let g2 = window.assign();
        let g3 = window.assign();
        assert_eq!(window.watermark_for_testing(), 0);

        // Out-of-order completion parks above the watermark until the gap fills.
        drop(g2);
        assert_eq!(window.watermark_for_testing(), 0);
        drop(g1);
        assert_eq!(window.watermark_for_testing(), 2);
        drop(g3);
        assert_eq!(window.watermark_for_testing(), 3);
    }

    #[test]
    fn group_index_is_done_when_last_clone_drops() {
        let window = CausalWindow::new(2, 8);
        let g1 = window.assign();
        let g1_clone = g1.clone();
        drop(g1);
        assert_eq!(window.watermark_for_testing(), 0);
        drop(g1_clone);
        assert_eq!(window.watermark_for_testing(), 1);
    }

    #[test]
    fn next_consecutive_is_always_admitted() {
        let window = CausalWindow::new(1, 8);
        let g1 = window.assign();
        let g2 = window.assign();

        // Fill the single concurrency slot with index 2.
        let _slot2 = window.try_admit(g2.index()).unwrap();
        // The window branch is closed, but index 1 == watermark+1 is still admitted.
        let slot1 = window.try_admit(g1.index()).unwrap();
        // A second member of the watermark+1 group is also admitted.
        let slot1b = window.try_admit(g1.index()).unwrap();

        drop(slot1);
        drop(slot1b);
        drop(g1);
        assert_eq!(window.watermark_for_testing(), 1);
        // Index 2 is now watermark+1; admission of another member ignores the limit.
        assert!(window.try_admit(g2.index()).is_some());
    }

    #[test]
    fn window_bounds_run_ahead() {
        let window = CausalWindow::new(8, 4);
        let mut guards: Vec<_> = (0..6).map(|_| window.assign()).collect();

        // Indices 1..4 are within the window (index < C + W = 4)...
        let _s1 = window.try_admit(guards[0].index()).unwrap();
        let _s2 = window.try_admit(guards[1].index()).unwrap();
        let _s3 = window.try_admit(guards[2].index()).unwrap();
        // ...but index 4 is not (4 < 0 + 4 fails, and it is not watermark+1).
        assert!(window.try_admit(guards[3].index()).is_none());

        // Retiring index 1 advances the watermark and extends the window.
        drop(_s1);
        drop(guards.remove(0));
        assert_eq!(window.watermark_for_testing(), 1);
        assert!(window.try_admit(guards[2].index()).is_some());
    }

    #[test]
    fn concurrency_limit_bounds_window_admissions() {
        let window = CausalWindow::new(2, 100);
        let guards: Vec<_> = (0..5).map(|_| window.assign()).collect();

        // Admit indices 2 and 3 through the window branch, filling the limit.
        let _s2 = window.try_admit(guards[1].index()).unwrap();
        let _s3 = window.try_admit(guards[2].index()).unwrap();
        assert!(window.try_admit(guards[3].index()).is_none());

        // Releasing a slot reopens the window branch.
        drop(_s2);
        assert!(window.try_admit(guards[3].index()).is_some());
    }

    #[test]
    fn retirement_without_admission_advances_watermark() {
        let window = CausalWindow::new(2, 8);
        let g1 = window.assign();
        let g2 = window.assign();
        // Unit 1 is dropped without ever being admitted (e.g. already executed).
        drop(g1);
        assert_eq!(window.watermark_for_testing(), 1);
        assert!(window.try_admit(g2.index()).is_some());
    }
}
