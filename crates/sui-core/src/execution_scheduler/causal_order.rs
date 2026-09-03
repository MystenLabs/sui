// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Causal-order bookkeeping for execution scheduling.
//!
//! Every unit the ExecutionScheduler enqueues is assigned a *causal index* - its
//! position in enqueue order. Both sources (consensus handler, checkpoint executor)
//! enqueue in causal order, so anything a transaction can wait for during execution
//! is produced by a unit with a lower index. The execution driver admits
//! transactions by index (see `execution_driver.rs`) and retires them - on completing
//! execution, or on dropping a stale transaction. (One exception: a funds-withdraw
//! whose scheduling is skipped retires at the scheduler, as it never reaches the
//! driver.)
//!
//! Enqueues are deduplicated by accumulator root version: all transactions of a
//! version are enqueued together, so a unit at or below the enqueue watermark was
//! already enqueued in full by the other source. [`CausalAdmission::dedup_and_assign`]
//! filters, indexes, and bumps the watermark for a whole batch under one lock,
//! serializing version admission across the two sources; combined with every
//! committed transaction executing before its epoch closes, this guarantees every
//! assigned index reaches the driver. Units without a version (pre-accumulator
//! epochs replayed from checkpoints, the end-of-epoch transaction) bypass
//! deduplication; execution cannot block on undeclared dependencies in such epochs,
//! and any future blocking feature must be gated on accumulator-versioned epochs.
//!
//! A unit that already-indexed units may wait on must never be re-assigned a new,
//! higher index (its blocked waiters would pin the concurrency limit while the
//! causal-next lane can never reach it). Hence the funds-withdraw retry keeps its
//! original index, and settlement transactions - which materialize long after their
//! waiters enqueue - are index-less: never blocked by admission, though they occupy
//! a concurrency slot when one is free.

use std::{collections::BTreeSet, sync::Arc};

use mysten_common::{assert_reachable, assert_sometimes};
use parking_lot::Mutex;
use sui_types::base_types::SequenceNumber;
use tokio::sync::Notify;

use crate::authority::ExecutionEnv;

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

    /// Deduplicates an enqueue batch and assigns causal indices to the survivors, in
    /// batch order, atomically: units whose assigned accumulator root version is at
    /// or below the enqueue watermark were already enqueued in full by the other
    /// source and are dropped; the watermark then rises to the batch's highest
    /// version. Units without an accumulator version are never deduplicated.
    ///
    /// Processing the whole batch under one lock serializes version admission across
    /// the consensus and checkpoint paths.
    pub fn dedup_and_assign<T>(&self, certs: Vec<(T, ExecutionEnv)>) -> Vec<(T, ExecutionEnv)> {
        // Internal re-submissions of already-indexed certificates must not pass
        // through here again - they keep their original index.
        debug_assert!(certs.iter().all(|(_, env)| env.causal_index.is_none()));
        let mut inner = self.inner.lock();
        let watermark = inner.enqueue_watermark;
        let mut max_version = watermark;
        let certs = certs
            .into_iter()
            .filter_map(|(item, mut env)| {
                if let Some(version) = env.assigned_versions.accumulator_version() {
                    if watermark.is_some_and(|w| version <= w) {
                        assert_reachable!("rejected duplicate enqueue of a version group");
                        return None;
                    }
                    max_version = max_version.max(Some(version));
                }
                env.causal_index = Some(inner.next_index);
                inner.next_index += 1;
                Some((item, env))
            })
            .collect();
        inner.enqueue_watermark = max_version;
        certs
    }

    /// Attempts to admit the transaction with `index` for execution. On success,
    /// returns a slot that must be held for the duration of execution; dropping it
    /// releases the concurrency slot and retires the index.
    ///
    /// Admission policy: under the concurrency limit, anything goes. At the limit, the
    /// next transaction in causal order is still admitted, one at a time - it can
    /// never block (everything below it is done), which is what guarantees progress no
    /// matter how many admitted transactions are parked. In-flight is thereby bounded
    /// at concurrency_limit + 1.
    pub fn try_admit(self: &Arc<Self>, index: u64) -> Option<InFlightSlot> {
        let mut inner = self.inner.lock();
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
            retire_on_drop: Some(index),
        })
    }

    /// Takes a concurrency slot if one is free, without any causal-index conditions.
    /// For settlement transactions: they must never be blocked by admission
    /// (transactions may be parked waiting on the very versions they write), but they
    /// respect the concurrency limit when there is room. The returned slot releases on
    /// drop and retires nothing.
    pub fn try_take_slot(self: &Arc<Self>) -> Option<InFlightSlot> {
        let mut inner = self.inner.lock();
        if inner.in_flight >= self.concurrency_limit {
            return None;
        }
        inner.in_flight += 1;
        Some(InFlightSlot {
            admission: self.clone(),
            is_next: false,
            retire_on_drop: None,
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
        let mut inner = self.inner.lock();
        Self::mark_done_locked(&mut inner, index);
        drop(inner);
        self.notify.notify_one();
    }

    fn mark_done_locked(inner: &mut AdmissionInner, index: u64) {
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
    }

    #[cfg(test)]
    pub fn watermark_for_testing(&self) -> u64 {
        self.inner.lock().watermark
    }
}

/// A held execution-concurrency slot. Dropping it releases the slot and retires the
/// admitted index - one lock and one driver wakeup for the whole completion.
pub struct InFlightSlot {
    admission: Arc<CausalAdmission>,
    is_next: bool,
    retire_on_drop: Option<u64>,
}

impl InFlightSlot {
    /// Keeps the admitted causal index alive past this execution attempt, for a
    /// transaction that will be re-submitted under the same index (RetryLater).
    pub fn skip_retire(&mut self) {
        self.retire_on_drop = None;
    }
}

impl Drop for InFlightSlot {
    fn drop(&mut self) {
        let mut inner = self.admission.inner.lock();
        inner.in_flight -= 1;
        if self.is_next {
            inner.next_admitted = false;
        }
        if let Some(index) = self.retire_on_drop {
            CausalAdmission::mark_done_locked(&mut inner, index);
        }
        drop(inner);
        self.admission.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::shared_object_version_manager::AssignedVersions;

    fn env(version: Option<u64>) -> ExecutionEnv {
        ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
            vec![],
            version.map(SequenceNumber::from_u64),
        ))
    }

    /// Enqueues `n` version-less units and returns their assigned indices.
    fn assign(admission: &CausalAdmission, n: usize) -> Vec<u64> {
        admission
            .dedup_and_assign((0..n).map(|_| ((), env(None))).collect())
            .into_iter()
            .map(|(_, env)| env.causal_index.unwrap())
            .collect()
    }

    #[test]
    fn watermark_advances_over_done_indices() {
        let admission = CausalAdmission::new(2);
        assert_eq!(assign(&admission, 3), vec![1, 2, 3]);
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

        // A batch with versions 5 and 6 is admitted in full.
        let batch = vec![((), env(Some(5))), ((), env(Some(5))), ((), env(Some(6)))];
        assert_eq!(admission.dedup_and_assign(batch).len(), 3);
        // Versions at or below the watermark are duplicates from the other source.
        let batch = vec![((), env(Some(5))), ((), env(Some(6)))];
        assert!(admission.dedup_and_assign(batch).is_empty());
        // A higher version is new work, and version-less units always pass.
        let batch = vec![((), env(Some(7))), ((), env(None))];
        let admitted = admission.dedup_and_assign(batch);
        assert_eq!(
            admitted
                .into_iter()
                .map(|(_, env)| env.causal_index.unwrap())
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
    }

    #[test]
    fn next_consecutive_is_admitted_at_the_limit() {
        let admission = CausalAdmission::new(1);
        assign(&admission, 2);

        // Fill the single concurrency slot with index 2.
        let mut slot2 = admission.try_admit(2).unwrap();
        // The capacity branch is closed, but index 1 == watermark+1 is still admitted.
        let mut slot1 = admission.try_admit(1).unwrap();
        // Only one causal-next admission may be outstanding at a time.
        assert!(admission.try_admit(1).is_none());
        // A RetryLater release keeps the index alive, and it can be admitted again.
        slot1.skip_retire();
        drop(slot1);
        let slot1b = admission.try_admit(1).unwrap();

        // Completing index 1 retires it via the slot drop.
        drop(slot1b);
        assert_eq!(admission.watermark_for_testing(), 1);
        // Index 2 is now watermark+1; re-admitting it bypasses the (full) limit.
        slot2.skip_retire();
        drop(slot2);
        let slot2b = admission.try_admit(2).unwrap();
        drop(slot2b);
        assert_eq!(admission.watermark_for_testing(), 2);
    }

    #[test]
    fn concurrency_limit_bounds_admissions() {
        let admission = CausalAdmission::new(2);
        assign(&admission, 5);

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
        assign(&admission, 2);
        // Unit 1 is dropped by the driver without being admitted (e.g. already
        // executed).
        admission.mark_done(1);
        assert_eq!(admission.watermark_for_testing(), 1);
        assert!(admission.try_admit(2).is_some());
    }
}
