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
//! Index lifecycle. Indices are assigned at enqueue time via [`CausalAdmission::admit_enqueue`]
//! and travel as plain data in the unit's `ExecutionEnv`. Every assigned index is
//! *retired* (marked done) by the execution driver - the sole execution and retirement
//! point - when its transaction finishes executing or is dropped as no longer needed.
//! This relies on every indexed unit reaching the driver: enqueued transactions are
//! deduplicated (below), and every transaction committed in an epoch executes before
//! the epoch closes, so no indexed unit is ever abandoned upstream. Settlement
//! transactions carry no index at all: the driver admits them unconditionally (they
//! can never block, and running them is what unblocks transactions waiting on the
//! accumulator versions they write), so there is nothing to admit against or retire.
//!
//! Deduplication. `admit_enqueue` admits or rejects an entire version group
//! atomically, keyed on the group's assigned accumulator root version: all
//! transactions of a root version are enqueued together (one consensus chunk, or one
//! whole group within a checkpoint), so a group at or below the enqueue watermark has
//! already been enqueued in full by whichever source got there first, and is rejected
//! without assigning indices. Performing the version check, index assignment and
//! watermark bump under one lock serializes version admission across the consensus and
//! checkpoint paths: a rejected group's transactions are guaranteed to already hold
//! lower indices. Units with no accumulator version bypass deduplication - that only
//! occurs when replaying epochs that predate accumulators (checkpoint path only, which
//! never self-duplicates) plus the end-of-epoch transaction; execution cannot block on
//! undeclared dependencies in such epochs, so any future blocking feature must be
//! gated on accumulator-versioned epochs.
//!
//! One rule must hold for assignment: a unit that other, already-indexed units may
//! wait on must never be re-assigned a *new, higher* index (transactions blocked
//! waiting on it would pin the concurrency limit while the causal-next lane can never
//! reach it). This is why the funds-withdraw retry re-submits with the transaction's
//! original index, and why settlement transactions - which materialize long after
//! their waiters enqueue - are admitted unconditionally rather than indexed late.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use mysten_common::assert_sometimes;
use sui_types::base_types::SequenceNumber;
use tokio::sync::Notify;

/// Shared state between the ExecutionScheduler (index assignment and enqueue
/// deduplication) and the execution driver (admission and retirement). See the module
/// comment and `execution_driver.rs`.
pub struct CausalAdmission {
    inner: Mutex<AdmissionInner>,
    /// Wakes the driver loop when the watermark or in-flight count changes.
    notify: Notify,
    /// Max transactions admitted for execution concurrently via the capacity branch (K).
    concurrency_limit: usize,
}

struct AdmissionInner {
    /// The next index to assign. Indices start at 1; watermark 0 means "nothing done".
    next_index: u64,
    /// The highest accumulator root version whose group has been fully enqueued.
    /// Groups at or below it are duplicates.
    enqueue_watermark: Option<SequenceNumber>,
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
    /// consensus, networking and checkpointing. In test configurations the limit is
    /// randomized instead - both because the host's CPU count must not influence
    /// simulation behavior, and to explore admission interleavings, including small
    /// limits that force transactions through the causal-next lane.
    pub fn new_with_default_sizing() -> Arc<Self> {
        let concurrency_limit = if mysten_common::in_test_configuration() {
            use rand::Rng;
            mysten_common::random::get_rng().gen_range(1..=8)
        } else {
            std::cmp::max(1, num_cpus::get() / 2)
        };
        tracing::info!("execution concurrency limit: {concurrency_limit}");
        Self::new(concurrency_limit)
    }

    pub fn new(concurrency_limit: usize) -> Arc<Self> {
        assert!(concurrency_limit > 0);
        Arc::new(Self {
            inner: Mutex::new(AdmissionInner {
                next_index: 1,
                enqueue_watermark: None,
                watermark: 0,
                done_above: BTreeSet::new(),
                in_flight: 0,
                next_admitted: false,
            }),
            notify: Notify::new(),
            concurrency_limit,
        })
    }

    /// Deduplicates and assigns indices for one externally-enqueued version group of
    /// `count` units, in enqueue order. Returns the first of `count` consecutive
    /// indices, or None if the group's version is at or below the enqueue watermark -
    /// meaning the whole group was already enqueued (with lower indices) by the other
    /// source. Groups without an accumulator version are never deduplicated.
    ///
    /// The check, assignment and watermark bump are atomic, serializing version
    /// admission across the consensus and checkpoint paths.
    pub fn admit_enqueue(&self, version: Option<SequenceNumber>, count: usize) -> Option<u64> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(version) = version {
            if inner.enqueue_watermark.is_some_and(|w| version <= w) {
                assert_sometimes!(true, "rejected duplicate enqueue of a version group");
                return None;
            }
            inner.enqueue_watermark = Some(version);
        }
        let first = inner.next_index;
        inner.next_index += count as u64;
        Some(first)
    }

    /// Attempts to admit the transaction with `index` for execution. On success,
    /// returns a slot that must be held for the duration of execution; dropping it
    /// releases the concurrency slot.
    ///
    /// Admission policy: under the concurrency limit, anything goes. At the limit, the
    /// next transaction in causal order is still admitted, one at a time - it can
    /// never block (everything below it is done), which is what guarantees progress no
    /// matter how many admitted transactions are parked. In-flight is thereby bounded
    /// at concurrency_limit + 1.
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
        assert_sometimes!(is_next, "admitted via the causal-next lane over the limit");
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

    /// Marks `index` done - its transaction finished executing, or was dropped as no
    /// longer needed. Called exactly once per index.
    pub fn mark_done(&self, index: u64) {
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
        let first = admission.admit_enqueue(None, 3).unwrap();
        assert_eq!(first, 1);
        assert_eq!(admission.watermark_for_testing(), 0);

        // Out-of-order completion parks above the watermark until the gap fills.
        admission.mark_done(2);
        assert_eq!(admission.watermark_for_testing(), 0);
        admission.mark_done(1);
        assert_eq!(admission.watermark_for_testing(), 2);
        admission.mark_done(3);
        assert_eq!(admission.watermark_for_testing(), 3);
    }

    #[test]
    fn duplicate_version_groups_are_rejected() {
        let admission = CausalAdmission::new(2);
        let v1 = SequenceNumber::from_u64(5);
        let v2 = SequenceNumber::from_u64(6);

        assert_eq!(admission.admit_enqueue(Some(v1), 3), Some(1));
        // The same version from the other source is a duplicate.
        assert_eq!(admission.admit_enqueue(Some(v1), 3), None);
        // A higher version is new work.
        assert_eq!(admission.admit_enqueue(Some(v2), 2), Some(4));
        // Groups without a version are never deduplicated.
        assert_eq!(admission.admit_enqueue(None, 1), Some(6));
        assert_eq!(admission.admit_enqueue(None, 1), Some(7));
    }

    #[test]
    fn next_consecutive_is_admitted_at_the_limit() {
        let admission = CausalAdmission::new(1);
        admission.admit_enqueue(None, 2).unwrap();

        // Fill the single concurrency slot with index 2.
        let _slot2 = admission.try_admit(2).unwrap();
        // The capacity branch is closed, but index 1 == watermark+1 is still admitted.
        let slot1 = admission.try_admit(1).unwrap();
        // Only one causal-next admission may be outstanding at a time.
        assert!(admission.try_admit(1).is_none());
        drop(slot1);
        // An unretired index (a RetryLater re-submission) can be admitted again.
        let slot1b = admission.try_admit(1).unwrap();

        drop(slot1b);
        admission.mark_done(1);
        assert_eq!(admission.watermark_for_testing(), 1);
        // Index 2 is now watermark+1; admitting again bypasses the (full) limit.
        assert!(admission.try_admit(2).is_some());
    }

    #[test]
    fn concurrency_limit_bounds_admissions() {
        let admission = CausalAdmission::new(2);
        admission.admit_enqueue(None, 5).unwrap();

        // Admit indices 2 and 3 through the capacity branch, filling the limit.
        let _s2 = admission.try_admit(2).unwrap();
        let _s3 = admission.try_admit(3).unwrap();
        // Index 4 is neither under the limit nor the causal-next transaction.
        assert!(admission.try_admit(4).is_none());

        // Releasing a slot reopens the capacity branch, regardless of how far the
        // index runs ahead of the watermark.
        drop(_s2);
        assert!(admission.try_admit(5).is_some());
    }

    #[test]
    fn retirement_without_admission_advances_watermark() {
        let admission = CausalAdmission::new(2);
        admission.admit_enqueue(None, 2).unwrap();
        // Unit 1 is dropped by the driver without being admitted (e.g. already
        // executed).
        admission.mark_done(1);
        assert_eq!(admission.watermark_for_testing(), 1);
        assert!(admission.try_admit(2).is_some());
    }
}
