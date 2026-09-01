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
//! Index lifecycle. [`CausalAdmission::assign`] hands out the next index wrapped in a
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
//! on must never be re-assigned a *new, higher* index (transactions blocked waiting on
//! it would pin the concurrency limit while the causal-next lane can never reach it).
//! Units that will execute later keep their original guard - a retry path clones the
//! env before execution finishes; only units that are truly done may drop it.

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
pub struct CausalAdmission {
    inner: Mutex<AdmissionInner>,
    /// Wakes the driver loop when the watermark or in-flight count changes.
    notify: Notify,
    /// The next index to assign. Indices start at 1; watermark 0 means "nothing done".
    next_index: AtomicU64,
    /// Max transactions admitted for execution concurrently via the capacity branch (K).
    concurrency_limit: usize,
}

struct AdmissionInner {
    /// The watermark C: every index <= C is done (executed or retired).
    watermark: u64,
    /// Done indices > C, awaiting the gap below them to fill.
    done_above: BTreeSet<u64>,
    /// Admitted (submitted for execution) transactions not yet finished. Includes
    /// transactions blocked inside execution.
    in_flight: usize,
    /// Whether a causal-next admission (over the concurrency limit) is outstanding.
    /// At most one at a time, bounding in-flight at concurrency_limit + 1.
    next_admitted: bool,
}

impl CausalAdmission {
    /// Default sizing: execution parallelism K at half the CPUs, leaving the rest for
    /// consensus, networking and checkpointing. Under msim K is fixed: host CPU count
    /// must not influence simulation behavior, and the sim's blocking pool (default 32
    /// threads, shared with other spawn_blocking users) must accommodate K + 1.
    pub fn new_with_default_sizing() -> Arc<Self> {
        #[cfg(msim)]
        return Self::new(4);
        #[cfg(not(msim))]
        Self::new(std::cmp::max(1, num_cpus::get() / 2))
    }

    pub fn new(concurrency_limit: usize) -> Arc<Self> {
        assert!(concurrency_limit > 0);
        Arc::new(Self {
            inner: Mutex::new(AdmissionInner {
                watermark: 0,
                done_above: BTreeSet::new(),
                in_flight: 0,
                next_admitted: false,
            }),
            notify: Notify::new(),
            next_index: AtomicU64::new(1),
            concurrency_limit,
        })
    }

    /// The number of pool threads needed so that admission never has to queue:
    /// in-flight never exceeds concurrency_limit plus the single outstanding
    /// causal-next admission.
    pub fn required_pool_size(&self) -> usize {
        self.concurrency_limit + 1
    }

    /// Assigns the next causal index. Callers must call this in causal order (the
    /// order units are enqueued from consensus handler / checkpoint executor).
    pub fn assign(self: &Arc<Self>) -> CausalIndexGuard {
        let index = self.next_index.fetch_add(1, Ordering::Relaxed);
        CausalIndexGuard(Arc::new(GuardInner {
            index,
            admission: self.clone(),
        }))
    }

    /// Attempts to admit the transaction with `index` for execution. On success,
    /// returns a slot that must be held for the duration of execution; dropping it
    /// releases the concurrency slot.
    ///
    /// Admission policy: under the concurrency limit, anything goes. At the limit, the
    /// next transaction in causal order is still admitted, one at a time - it can
    /// never block (everything below it is done) and a pool thread is always free for
    /// it, which is what guarantees progress no matter how many admitted transactions
    /// are parked.
    pub fn try_admit(self: &Arc<Self>, index: u64) -> Option<InFlightSlot> {
        let mut inner = self.inner.lock().unwrap();
        debug_assert!(index > inner.watermark, "admitting an already-done index");
        let is_next = if inner.in_flight < self.concurrency_limit {
            false
        } else if index == inner.watermark + 1 && !inner.next_admitted {
            inner.next_admitted = true;
            true
        } else {
            return None;
        };
        inner.in_flight += 1;
        Some(InFlightSlot {
            admission: self.clone(),
            is_next,
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
    admission: Arc<CausalAdmission>,
}

impl Drop for GuardInner {
    fn drop(&mut self) {
        self.admission.mark_done(self.index);
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
    admission: Arc<CausalAdmission>,
    is_next: bool,
}

impl Drop for InFlightSlot {
    fn drop(&mut self) {
        let mut inner = self.admission.inner.lock().unwrap();
        inner.in_flight -= 1;
        if self.is_next {
            inner.next_admitted = false;
        }
        drop(inner);
        self.admission.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watermark_advances_over_done_indices() {
        let admission = CausalAdmission::new(2);
        let g1 = admission.assign();
        let g2 = admission.assign();
        let g3 = admission.assign();
        assert_eq!(admission.watermark_for_testing(), 0);

        // Out-of-order completion parks above the watermark until the gap fills.
        drop(g2);
        assert_eq!(admission.watermark_for_testing(), 0);
        drop(g1);
        assert_eq!(admission.watermark_for_testing(), 2);
        drop(g3);
        assert_eq!(admission.watermark_for_testing(), 3);
    }

    #[test]
    fn group_index_is_done_when_last_clone_drops() {
        let admission = CausalAdmission::new(2);
        let g1 = admission.assign();
        let g1_clone = g1.clone();
        drop(g1);
        assert_eq!(admission.watermark_for_testing(), 0);
        drop(g1_clone);
        assert_eq!(admission.watermark_for_testing(), 1);
    }

    #[test]
    fn next_consecutive_is_admitted_at_the_limit() {
        let admission = CausalAdmission::new(1);
        let g1 = admission.assign();
        let g2 = admission.assign();

        // Fill the single concurrency slot with index 2.
        let _slot2 = admission.try_admit(g2.index()).unwrap();
        // The capacity branch is closed, but index 1 == watermark+1 is still admitted.
        let slot1 = admission.try_admit(g1.index()).unwrap();
        // Only one causal-next admission may be outstanding, so a second member of the
        // watermark+1 group must wait for it.
        assert!(admission.try_admit(g1.index()).is_none());
        drop(slot1);
        let slot1b = admission.try_admit(g1.index()).unwrap();

        drop(slot1b);
        drop(g1);
        assert_eq!(admission.watermark_for_testing(), 1);
        // Index 2 is now watermark+1; admitting another member of it again bypasses
        // the (full) concurrency limit.
        assert!(admission.try_admit(g2.index()).is_some());
    }

    #[test]
    fn concurrency_limit_bounds_admissions() {
        let admission = CausalAdmission::new(2);
        let guards: Vec<_> = (0..5).map(|_| admission.assign()).collect();

        // Admit indices 2 and 3 through the capacity branch, filling the limit.
        let _s2 = admission.try_admit(guards[1].index()).unwrap();
        let _s3 = admission.try_admit(guards[2].index()).unwrap();
        // Index 4 is neither under the limit nor the causal-next transaction.
        assert!(admission.try_admit(guards[3].index()).is_none());

        // Releasing a slot reopens the capacity branch, regardless of how far the
        // index runs ahead of the watermark.
        drop(_s2);
        assert!(admission.try_admit(guards[4].index()).is_some());
    }

    #[test]
    fn retirement_without_admission_advances_watermark() {
        let admission = CausalAdmission::new(2);
        let g1 = admission.assign();
        let g2 = admission.assign();
        // Unit 1 is dropped without ever being admitted (e.g. already executed).
        drop(g1);
        assert_eq!(admission.watermark_for_testing(), 1);
        assert!(admission.try_admit(g2.index()).is_some());
    }
}
