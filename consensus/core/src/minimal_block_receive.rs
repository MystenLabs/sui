// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Receive-side recovery for minimal blocks that cannot be inflated at receipt —
//! block-manager suspension, transposed to slots.
//!
//! Each un-inflatable minimal block becomes one bounded task mirroring what
//! suspension does for full blocks: it holds the claim, registers on the COMPLETE
//! missing-slot frontier via `DagState`'s accepted-slot notifier, and durably
//! registers the claimed reference with the synchronizer — the same component that
//! fetches suspended blocks' missing ancestors. At tip the frontier usually fills
//! from the streams within milliseconds and the registration is pruned before the
//! periodic scheduler fetches it (a pass that snapshots the set just before local
//! acceptance can still spend one redundant fetch); for a receiver that is behind,
//! the scheduler fetches the full block on its existing cadence and the accepted
//! result wakes the task through its exact-reference registration. Registrations are
//! released by acceptance through ANY path or by GC, so a block that arrives via
//! commit sync or replay costs nothing further. Recovery needs no timers of its own.
//!
//! Termination: the frontier fills (inflate + submit), the claimed block arrives
//! through another path (release, delivering the excluded-ancestors sidecar), or GC
//! passes the claimed round (obsolete). All quota admission is per-peer only and
//! charges resident memory — payload, sidecar, task overhead, and notifier
//! registrations — sized so honest traffic can never approach the caps.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use itertools::Itertools as _;
use mysten_common::debug_fatal;
use parking_lot::RwLock;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::debug;

use crate::{
    block::Slot,
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::ConsensusResult,
    minimal_block::{FallbackReason, InflateError},
    network::{ExtendedSerializedBlock, ValidatorNetworkService},
    synchronizer::SynchronizerHandle,
};

/// Per-peer admission caps on resident recovery memory. Honest steady state is a few
/// hundred KB per peer (stream rate x GC window); these are ~100x anti-fabrication
/// backstops — parked claims carry no verifiable signature yet, so unlike suspended
/// blocks their creation costs an attacker nothing. A quota hit indicates an attack
/// (or a bug) and resets that peer's stream; it must never fire for honest traffic.
const MAX_PARKED_BYTES_PER_PEER: usize = 32 << 20;
const MAX_PARKED_BLOCKS_PER_PEER: usize = 2048;

/// Resident-memory estimate charged per admission, beyond the wire payloads:
/// task/future/envelope overhead plus one notifier registration per frontier slot.
const TASK_OVERHEAD_BYTES: usize = 2048;
const REGISTRATION_CHARGE_BYTES: usize = 256;

/// Cap on untrusted minimal bytes, enforced before ANY decoding: a legitimate
/// minimal block is bounded by the max transaction payload plus small per-ancestor
/// structure.
pub(crate) fn max_minimal_size(context: &Context) -> usize {
    (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2)
}

/// Upper bound on a legitimate excluded-ancestors sidecar: the receive path caps the
/// list at twice the committee size, each a serialized `BlockRef`.
pub(crate) fn max_excluded_ancestors_size(context: &Context) -> usize {
    const SERIALIZED_BLOCK_REF_BYTES: usize = 64;
    context
        .committee
        .size()
        .saturating_mul(2)
        .saturating_mul(SERIALIZED_BLOCK_REF_BYTES)
}

/// Resident-memory charge for admitting one parked block.
pub(crate) fn admission_charge(
    minimal_len: usize,
    excluded_ancestors_len: usize,
    frontier_len: usize,
) -> usize {
    minimal_len
        .saturating_add(excluded_ancestors_len)
        .saturating_add(TASK_OVERHEAD_BYTES)
        .saturating_add(frontier_len.saturating_mul(REGISTRATION_CHARGE_BYTES))
}

/// Which per-peer bound rejected a recovery admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotaLimit {
    PeerBytes,
    PeerCount,
}

impl QuotaLimit {
    pub(crate) fn label(self) -> &'static str {
        match self {
            QuotaLimit::PeerBytes => "peer_bytes",
            QuotaLimit::PeerCount => "peer_count",
        }
    }
}

/// Quota bounds, overridable in tests to force capacity pressure.
pub(crate) struct RecoveryLimits {
    pub(crate) peer_bytes: usize,
    pub(crate) peer_count: usize,
}

impl Default for RecoveryLimits {
    fn default() -> Self {
        Self {
            peer_bytes: MAX_PARKED_BYTES_PER_PEER,
            peer_count: MAX_PARKED_BLOCKS_PER_PEER,
        }
    }
}

struct PeerQuota {
    bytes: Arc<Semaphore>,
    count: Arc<Semaphore>,
}

/// Per-peer admission control. Acquisition is non-blocking and happens before the
/// task is spawned; the RAII permit releases everything — and corrects the parked
/// gauges — on success, error, panic, task abort, and shutdown alike.
pub(crate) struct RecoveryQuotas {
    context: Arc<Context>,
    peer_bytes_limit: usize,
    per_peer: Vec<PeerQuota>,
}

impl RecoveryQuotas {
    pub(crate) fn new(context: Arc<Context>) -> Arc<Self> {
        // The per-peer byte quota must always admit ONE maximum-charge legitimate
        // block — payload, sidecar, overhead and a full-committee frontier — whatever
        // the protocol's size caps become, or such a block could never be recovered.
        let max_charge = admission_charge(
            max_minimal_size(&context),
            max_excluded_ancestors_size(&context),
            context.committee.size(),
        );
        let limits = RecoveryLimits {
            peer_bytes: MAX_PARKED_BYTES_PER_PEER.max(max_charge),
            ..RecoveryLimits::default()
        };
        Self::with_limits(context, limits)
    }

    pub(crate) fn with_limits(context: Arc<Context>, limits: RecoveryLimits) -> Arc<Self> {
        let per_peer = (0..context.committee.size())
            .map(|_| PeerQuota {
                bytes: Arc::new(Semaphore::new(limits.peer_bytes)),
                count: Arc::new(Semaphore::new(limits.peer_count)),
            })
            .collect();
        Arc::new(Self {
            context,
            peer_bytes_limit: limits.peer_bytes,
            per_peer,
        })
    }

    pub(crate) fn try_acquire(
        self: &Arc<Self>,
        peer: AuthorityIndex,
        charge: usize,
    ) -> Result<RecoveryPermit, QuotaLimit> {
        // A charge larger than the per-peer byte quota can never be admitted; report
        // it as the byte limit rather than deadlocking on an unsatisfiable acquire.
        if charge > self.peer_bytes_limit {
            return Err(QuotaLimit::PeerBytes);
        }
        let charge_u32 = u32::try_from(charge).map_err(|_| QuotaLimit::PeerBytes)?;
        let quota = &self.per_peer[peer];
        let peer_bytes = quota
            .bytes
            .clone()
            .try_acquire_many_owned(charge_u32)
            .map_err(|_| QuotaLimit::PeerBytes)?;
        let peer_count = quota
            .count
            .clone()
            .try_acquire_owned()
            .map_err(|_| QuotaLimit::PeerCount)?;
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.minimal_block_recovery_parked.inc();
        node_metrics
            .minimal_block_recovery_parked_bytes
            .add(charge as i64);
        Ok(RecoveryPermit {
            context: self.context.clone(),
            charge,
            _peer_bytes: peer_bytes,
            _peer_count: peer_count,
        })
    }
}

/// Held for the lifetime of one recovery task. The parked gauges mirror permit
/// existence exactly: incremented at acquisition, decremented on drop.
pub(crate) struct RecoveryPermit {
    context: Arc<Context>,
    charge: usize,
    _peer_bytes: OwnedSemaphorePermit,
    _peer_count: OwnedSemaphorePermit,
}

impl Drop for RecoveryPermit {
    fn drop(&mut self) {
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.minimal_block_recovery_parked.dec();
        node_metrics
            .minimal_block_recovery_parked_bytes
            .sub(self.charge as i64);
    }
}

/// Durable registration of an exact missing block with the fetching machinery. In
/// production this is the synchronizer's periodic scheduler; tests substitute a
/// recorder.
#[async_trait]
pub(crate) trait MissingBlockRegistry: Send + Sync + 'static {
    async fn register_missing_block(&self, block_ref: BlockRef) -> ConsensusResult<()>;
}

#[async_trait]
impl MissingBlockRegistry for SynchronizerHandle {
    async fn register_missing_block(&self, block_ref: BlockRef) -> ConsensusResult<()> {
        SynchronizerHandle::register_missing_block(self, block_ref).await
    }
}

/// Terminal outcome labels for the `minimal_block_recoveries` metric.
enum RecoveryOutcome {
    Inflated,
    Obsolete,
    AlreadyAccepted,
}

impl RecoveryOutcome {
    fn label(&self) -> &'static str {
        match self {
            RecoveryOutcome::Inflated => "inflated",
            RecoveryOutcome::Obsolete => "obsolete",
            RecoveryOutcome::AlreadyAccepted => "already_accepted",
        }
    }
}

/// What the register-recheck under one DagState read guard concluded.
enum WaitCheck<'a> {
    /// The claimed block itself fell below GC: nothing to recover.
    Obsolete,
    /// The claimed block was accepted through another path (synchronizer fetch or
    /// full-form replay): recovery is redundant — release quota, deliver the sidecar.
    AlreadyAccepted,
    /// Every slot in the frontier has an accepted candidate: retry inflation now.
    Retry,
    /// A required slot fell below GC while the child is live: the frontier can never
    /// complete, so only the registered synchronizer fetch (or replay) can finish
    /// this block. Wait on the exact reference and GC alone.
    FrontierDead,
    /// Await the registrations of the still-empty slots as one barrier. Carries the
    /// lowest still-missing round for the GC wake threshold.
    Wait(
        Vec<mysten_common::sync::notify_read::Registration<'a, Slot, ()>>,
        Round,
    ),
}

/// Recovers one un-inflatable minimal block, starting from the receipt-time
/// classification (`reason`). Spawned (quota-first) by the subscriber; aborted with
/// its owning `JoinSet` on shutdown, which releases the permit and deregisters any
/// pending waits. The claimed reference is durably registered with the synchronizer,
/// which fetches it on its periodic cadence for as long as the block stays missing
/// and above GC — the task itself never fetches and never times anything.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn recover_minimal_block<S: ValidatorNetworkService>(
    context: Arc<Context>,
    block_inflater: Arc<BlockInflater>,
    dag_state: Weak<RwLock<DagState>>,
    registry: Arc<dyn MissingBlockRegistry>,
    authority_service: Weak<S>,
    peer: AuthorityIndex,
    claimed_ref: BlockRef,
    reason: FallbackReason,
    minimal: Bytes,
    excluded_ancestors: Vec<Vec<u8>>,
    permit: RecoveryPermit,
) {
    let _permit = permit;
    let parked_at = std::time::Instant::now();
    let node_metrics = &context.metrics.node_metrics;
    let (accepted_slots, accepted_refs, mut gc_round) = {
        let Some(dag_state) = dag_state.upgrade() else {
            return;
        };
        let dag_state = dag_state.read();
        (
            dag_state.accepted_slot_notifier(),
            dag_state.accepted_ref_notifier(),
            dag_state.gc_round_receiver(),
        )
    };

    // Durably register the claimed reference for fetching. At tip the block resolves
    // from the streams before the periodic scheduler's next pass and the registration
    // is pruned unfetched; behind the tip, the scheduler fetches the full block on
    // its existing cadence. The awaited send backpressures instead of dropping; a
    // shutdown error just ends the task (the permit releases via RAII).
    if registry.register_missing_block(claimed_ref).await.is_err() {
        return;
    }

    let mut reason = reason;
    let outcome = 'recovery: {
        loop {
            let missing = match &reason {
                FallbackReason::MissingAncestors(slots) => slots.clone(),
                // Ambiguity or a digest mismatch cannot be repaired by waiting on
                // slots; the registered synchronizer fetch (or replay) resolves the
                // block, so wait on the exact reference and GC alone.
                FallbackReason::AmbiguousSlot(_) | FallbackReason::DigestMismatch => vec![],
            };
            if !missing.is_empty() {
                node_metrics
                    .minimal_block_park_missing_slots
                    .observe(missing.len() as f64);
            }

            // Register BEFORE re-checking the DAG, so an acceptance between the check
            // and the await has already fired its registration: the frontier slots as
            // one barrier, and the claimed block's exact reference so the task ends
            // promptly when the synchronizer fetch or full-form replay wins.
            'wait: loop {
                let registrations = accepted_slots.register_all(&missing);
                let mut own_registration = accepted_refs.register_one(&claimed_ref);
                let check = {
                    let Some(dag_state) = dag_state.upgrade() else {
                        return;
                    };
                    let dag_state = dag_state.read();
                    let gc = dag_state.gc_round();
                    if claimed_ref.round <= gc {
                        WaitCheck::Obsolete
                    } else if dag_state.contains_block(&claimed_ref) {
                        WaitCheck::AlreadyAccepted
                    } else {
                        let mut remaining = Vec::new();
                        let mut lowest_missing = Round::MAX;
                        let mut crossed_gc = false;
                        for (slot, registration) in missing.iter().zip_eq(registrations) {
                            if !dag_state.get_uncommitted_blocks_at_slot(*slot).is_empty() {
                                // Already filled: dropping deregisters it.
                            } else if slot.round <= gc {
                                crossed_gc = true;
                            } else {
                                lowest_missing = lowest_missing.min(slot.round);
                                remaining.push(registration);
                            }
                        }
                        if crossed_gc || missing.is_empty() {
                            // Either a required slot can never fill, or the reason
                            // (ambiguity/mismatch) never had a frontier to wait on.
                            WaitCheck::FrontierDead
                        } else if remaining.is_empty() {
                            WaitCheck::Retry
                        } else {
                            WaitCheck::Wait(remaining, lowest_missing)
                        }
                    }
                };
                match check {
                    WaitCheck::Obsolete => break 'recovery RecoveryOutcome::Obsolete,
                    WaitCheck::AlreadyAccepted => {
                        break 'recovery RecoveryOutcome::AlreadyAccepted;
                    }
                    WaitCheck::Retry => break 'wait,
                    WaitCheck::FrontierDead => {
                        // Nothing slot-shaped left to wait for: only the fetched or
                        // replayed full block (own-ref wake) or GC can end this.
                        tokio::select! {
                            _ = &mut own_registration => continue 'wait,
                            changed = gc_round
                                .wait_for(|gc| *gc >= claimed_ref.round) => {
                                if changed.is_err() {
                                    return;
                                }
                                continue 'wait;
                            }
                        }
                    }
                    WaitCheck::Wait(remaining, lowest_missing) => {
                        let frontier = futures::future::join_all(remaining);
                        tokio::pin!(frontier);
                        // The GC arm wakes only when GC crosses a round the wait
                        // actually depends on — not on every commit tick.
                        let gc_threshold = lowest_missing.min(claimed_ref.round);
                        tokio::select! {
                            _ = &mut frontier => break 'wait,
                            _ = &mut own_registration => continue 'wait,
                            changed = gc_round.wait_for(move |gc| *gc >= gc_threshold) => {
                                if changed.is_err() {
                                    return;
                                }
                                continue 'wait;
                            }
                        }
                    }
                }
            }

            // The whole frontier has candidates: re-classify by inflating.
            let inflated = {
                let Some(dag_state) = dag_state.upgrade() else {
                    return;
                };
                let dag_state = dag_state.read();
                block_inflater.inflate(&minimal, peer, &dag_state)
            };
            match inflated {
                Ok((_signed, serialized)) => {
                    let Some(service) = authority_service.upgrade() else {
                        return;
                    };
                    let block = ExtendedSerializedBlock {
                        block: serialized,
                        minimal: None,
                        excluded_ancestors: excluded_ancestors.clone(),
                    };
                    match service.handle_send_block(peer, block).await {
                        Ok(()) => break 'recovery RecoveryOutcome::Inflated,
                        Err(error) => {
                            // Typically commit-lagging admission control. The block is
                            // still registered for fetching, so keep the task alive to
                            // deliver the sidecar when another path accepts it — and
                            // to retry once local state moves. Terminates on
                            // acceptance, GC, or shutdown like any other wait.
                            debug!(
                                "Recovered minimal block {} from peer {} was rejected: {}; \
                                 awaiting acceptance through the registered fetch",
                                claimed_ref, peer, error
                            );
                            node_metrics
                                .minimal_block_recoveries
                                .with_label_values(&["submit_rejected"])
                                .inc();
                            // Nothing slot-shaped left to do: wait for the exact block.
                            reason = FallbackReason::DigestMismatch;
                        }
                    }
                }
                Err(InflateError::NeedFullBlock {
                    reason: next_reason,
                    ..
                }) => {
                    reason = next_reason;
                }
                Err(InflateError::Malformed(error)) => {
                    // Receipt-time inflation already parsed these exact bytes, so a
                    // structural failure here is an invariant violation, not a peer
                    // fault — and it cannot improve with later DAG state either way.
                    debug_fatal!(
                        "malformed minimal block {} from peer {} in recovery after \
                         passing receipt-time parse: {}",
                        claimed_ref,
                        peer,
                        error
                    );
                    return;
                }
            }
        }
    };

    // A block accepted through another path still owes its propagation hints: the
    // fetched or replayed form arrived without the sidecar.
    if matches!(outcome, RecoveryOutcome::AlreadyAccepted)
        && !excluded_ancestors.is_empty()
        && let Some(service) = authority_service.upgrade()
    {
        let _ = service
            .handle_excluded_ancestors(peer, claimed_ref, excluded_ancestors)
            .await;
    }

    if let RecoveryOutcome::Inflated = outcome {
        node_metrics
            .minimal_block_recovery_latency
            .observe(parked_at.elapsed().as_secs_f64());
    }
    node_metrics
        .minimal_block_recoveries
        .with_label_values(&[outcome.label()])
        .inc();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use parking_lot::Mutex;

    use super::*;
    use crate::{
        VerifiedBlock,
        block::{BlockAPI as _, TestBlock},
        commit::{CommitDigest, TrustedCommit},
        network::test_network::TestService,
        storage::mem_store::MemStore,
    };
    use consensus_types::block::BlockDigest;

    /// Records durable registrations; the real synchronizer is exercised by the
    /// committee and simulation tests.
    #[derive(Default)]
    struct RecordingRegistry {
        registered: Mutex<Vec<BlockRef>>,
    }

    #[async_trait]
    impl MissingBlockRegistry for RecordingRegistry {
        async fn register_missing_block(&self, block_ref: BlockRef) -> ConsensusResult<()> {
            self.registered.lock().push(block_ref);
            Ok(())
        }
    }

    /// One recovery-task scenario: a receiver DAG, a peer block whose minimal form
    /// misses its non-own-parent ancestors, and everything needed to spawn the task.
    struct TaskHarness {
        context: Arc<Context>,
        peer: AuthorityIndex,
        receiver_dag: Arc<RwLock<DagState>>,
        inflater: Arc<BlockInflater>,
        service: Arc<Mutex<TestService>>,
        quotas: Arc<RecoveryQuotas>,
        registry: Arc<RecordingRegistry>,
        // Ancestors of `block`, NOT accepted into the receiver DAG.
        ancestors: Vec<VerifiedBlock>,
        block: VerifiedBlock,
        minimal: Bytes,
        excluded: Vec<Vec<u8>>,
        // Receipt-time classification against the (empty) receiver DAG, captured at
        // construction exactly as the subscriber captures it before spawning. Tests
        // that accept blocks before spawn exercise the stale-reason race on purpose.
        reason: FallbackReason,
    }

    fn harness(gc_depth: Option<u32>) -> TaskHarness {
        let (mut context, _keys) = Context::new_for_test(4);
        if let Some(depth) = gc_depth {
            context.protocol_config.set_gc_depth_for_testing(depth);
        }
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();
        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut ancestors = Vec::new();
        let mut ancestor_refs = Vec::new();
        for authority in 0..4u32 {
            let block = VerifiedBlock::new_for_test(TestBlock::new(10, authority).build());
            ancestor_refs.push(block.reference());
            sender_dag.write().accept_block(block.clone());
            ancestors.push(block);
        }
        ancestor_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone());
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(11, peer.value() as u32)
                .set_ancestors_raw(ancestor_refs)
                .build(),
        );
        let minimal = sender_inflater
            .serialize(&block, &sender_dag.read())
            .unwrap();
        // A propagation-hint sidecar that must survive every recovery path.
        let excluded = vec![
            bcs::to_bytes(&BlockRef::new(
                9,
                context.committee.to_authority_index(1).unwrap(),
                BlockDigest::MIN,
            ))
            .unwrap(),
        ];

        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let inflater = Arc::new(BlockInflater::new(context.clone()));
        let reason = match inflater
            .inflate(&minimal, peer, &receiver_dag.read())
            .map(|_| ())
        {
            Err(InflateError::NeedFullBlock { reason, .. }) => reason,
            other => panic!("scenario block must be un-inflatable at receipt: {other:?}"),
        };
        TaskHarness {
            peer,
            receiver_dag,
            inflater,
            service: Arc::new(Mutex::new(TestService::new())),
            quotas: RecoveryQuotas::new(context.clone()),
            registry: Arc::new(RecordingRegistry::default()),
            context,
            ancestors,
            block,
            minimal,
            excluded,
            reason,
        }
    }

    impl TaskHarness {
        fn spawn(&self, join_set: &mut tokio::task::JoinSet<()>) {
            let frontier_len = match &self.reason {
                FallbackReason::MissingAncestors(slots) => slots.len(),
                _ => 0,
            };
            let charge = admission_charge(
                self.minimal.len(),
                self.excluded.iter().map(|a| a.len()).sum(),
                frontier_len,
            );
            let permit = self.quotas.try_acquire(self.peer, charge).unwrap();
            join_set.spawn(recover_minimal_block(
                self.context.clone(),
                self.inflater.clone(),
                Arc::downgrade(&self.receiver_dag),
                self.registry.clone(),
                Arc::downgrade(&self.service),
                self.peer,
                self.block.reference(),
                self.reason.clone(),
                self.minimal.clone(),
                self.excluded.clone(),
                permit,
            ));
        }
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not reached in time");
    }

    /// The register-then-recheck ordering must close the race at every boundary,
    /// including acceptance strictly before the task starts (stale receipt reason).
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_closes_register_then_accept_race() {
        {
            let h = harness(None);
            h.receiver_dag.write().accept_blocks(h.ancestors.clone());
            let mut join_set = tokio::task::JoinSet::new();
            h.spawn(&mut join_set);
            wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        }
        for delay_ms in [0u64, 1, 5, 25] {
            let h = harness(None);
            let mut join_set = tokio::task::JoinSet::new();
            h.spawn(&mut join_set);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            h.receiver_dag.write().accept_blocks(h.ancestors.clone());
            wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
            let received = h.service.lock().handle_send_block.clone();
            assert_eq!(&received[0].1.block, h.block.serialized(), "{delay_ms}ms");
        }
    }

    /// The task registers the claimed reference durably at spawn (the synchronizer
    /// fetches it if the streams never resolve the frontier), waits on the WHOLE
    /// frontier as one barrier — partial arrivals neither re-inflate nor submit —
    /// and the final arrival produces exactly one submission carrying the sidecar.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_waits_for_full_frontier_then_inflates_once() {
        let h = harness(None);
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert_eq!(
            h.registry.registered.lock().as_slice(),
            &[h.block.reference()],
            "claimed ref must be durably registered for fetching"
        );

        let frontier: Vec<_> = h
            .ancestors
            .iter()
            .filter(|b| b.author() != h.peer)
            .cloned()
            .collect();
        let (last, first) = frontier.split_last().unwrap();
        for ancestor in first {
            h.receiver_dag.write().accept_block(ancestor.clone());
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert!(h.service.lock().handle_send_block.is_empty());
        }
        h.receiver_dag.write().accept_block(last.clone());
        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, h.block.serialized());
        // The propagation-hint sidecar rides the recovered submission.
        assert_eq!(received[0].1.excluded_ancestors, h.excluded);
    }

    /// Wrong siblings filling every frontier slot complete the barrier but fail the
    /// digest gate; the task then waits passively for the registered fetch or replay
    /// to deliver the exact block — it never submits a wrong reconstruction, and the
    /// sidecar is still delivered when the exact block arrives elsewhere.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_never_submits_wrong_reconstruction() {
        let h = harness(None);
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;

        let siblings: Vec<_> = h
            .ancestors
            .iter()
            .filter(|b| b.author() != h.peer)
            .map(|b| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, b.author().value() as u32)
                        .set_timestamp_ms(999_999)
                        .build(),
                )
            })
            .collect();
        h.receiver_dag.write().accept_blocks(siblings);
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Digest mismatch: nothing submitted, nothing wrong accepted.
        assert!(h.service.lock().handle_send_block.is_empty());

        // The registered fetch (or replay) wins: exact block accepted elsewhere.
        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| {
            h.context
                .metrics
                .node_metrics
                .minimal_block_recoveries
                .with_label_values(&["already_accepted"])
                .get()
                == 1
        })
        .await;
        assert!(h.service.lock().handle_send_block.is_empty());
        // Sidecar parity: the hints arrive through the dedicated path instead.
        let delivered = h.service.lock().handle_excluded_ancestors.clone();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].1, h.block.reference());
        assert_eq!(delivered[0].2, h.excluded);
    }

    /// GC crossing ONE frontier slot makes the frontier permanently incompletable:
    /// the task stops slot-waiting and ends only via the exact block arriving
    /// (registered fetch/replay) or its own round crossing GC.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_frontier_death_waits_for_exact_block() {
        let h = harness(Some(3));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;

        let filled: Vec<_> = h
            .ancestors
            .iter()
            .filter(|b| b.author() != h.peer)
            .take(2)
            .cloned()
            .collect();
        h.receiver_dag.write().accept_blocks(filled);
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Commit with leader round 13: gc_round = 10 kills the unfilled slot while
        // the claimed block (11) stays live.
        let leader = BlockRef::new(13, h.peer, BlockDigest::MIN);
        let commit = TrustedCommit::new_for_test(1, CommitDigest::MIN, 100, leader, vec![leader]);
        h.receiver_dag.write().add_commit(commit);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // The fetched exact block arrives: task ends AlreadyAccepted with sidecar.
        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| !h.service.lock().handle_excluded_ancestors.is_empty()).await;
        assert!(h.service.lock().handle_send_block.is_empty());
    }

    /// GC crossing the CLAIMED block itself makes recovery pointless: no submission,
    /// outcome obsolete, and no sidecar (the block never existed locally). The single
    /// commit crosses the whole frontier AND the child together, pinning the
    /// precedence of Obsolete over frontier death.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_drops_child_when_child_itself_is_gced() {
        let h = harness(Some(3));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;

        let leader = BlockRef::new(20, h.peer, BlockDigest::MIN);
        let commit = TrustedCommit::new_for_test(1, CommitDigest::MIN, 100, leader, vec![leader]);
        h.receiver_dag.write().add_commit(commit);

        wait_until(|| {
            h.context
                .metrics
                .node_metrics
                .minimal_block_recoveries
                .with_label_values(&["obsolete"])
                .get()
                == 1
        })
        .await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert!(h.service.lock().handle_excluded_ancestors.is_empty());
    }

    /// Replay can win before the task even starts: with the claimed block already
    /// accepted at spawn, the first register-recheck concludes AlreadyAccepted,
    /// releases the permit, and still delivers the sidecar. A sibling in the own
    /// slot must NOT trigger this.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_terminates_when_claimed_block_already_accepted_at_spawn() {
        let h = harness(None);
        h.receiver_dag.write().accept_block(h.block.clone());
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);

        let node_metrics = &h.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert_eq!(h.service.lock().handle_excluded_ancestors.len(), 1);
    }

    /// A sibling in the claimed slot must not wake or terminate the task: the
    /// termination registration is keyed by exact reference.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_ignores_equivocating_sibling_in_own_slot() {
        let h = harness(None);
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;
        let node_metrics = &h.context.metrics.node_metrics;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);

        let sibling = VerifiedBlock::new_for_test(
            TestBlock::new(11, h.peer.value() as u32)
                .set_timestamp_ms(777_777)
                .build(),
        );
        h.receiver_dag.write().accept_block(sibling);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);

        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
    }

    /// Aborting the owning JoinSet mid-wait releases the quota permit, corrects the
    /// gauges, and deregisters every notifier registration.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_shutdown_aborts_waiters_and_releases_quota() {
        let h = harness(None);
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;

        let node_metrics = &h.context.metrics.node_metrics;
        let slot_notifier = h.receiver_dag.read().accepted_slot_notifier();
        let ref_notifier = h.receiver_dag.read().accepted_ref_notifier();
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);
        // One registration per frontier slot (three non-own ancestors) plus the
        // claimed block's exact reference.
        assert_eq!(slot_notifier.num_pending(), 3);
        assert_eq!(ref_notifier.num_pending(), 1);

        join_set.shutdown().await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 0);
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
        assert_eq!(slot_notifier.num_pending(), 0);
        assert_eq!(ref_notifier.num_pending(), 0);
        let charge = admission_charge(h.minimal.len(), 0, 3);
        let permit = h.quotas.try_acquire(h.peer, charge).unwrap();
        drop(permit);
    }

    /// A rejected submission (commit-lagging admission control) must NOT end
    /// recovery: the block stays registered for fetching, and when another path
    /// accepts it the task still delivers the propagation-hint sidecar.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_survives_submission_rejection_and_delivers_sidecar() {
        let h = harness(None);
        h.service.lock().reject_send_block = true;
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set);
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Complete the frontier: the task inflates and submits, and is rejected.
        h.receiver_dag.write().accept_blocks(h.ancestors.clone());
        wait_until(|| {
            h.context
                .metrics
                .node_metrics
                .minimal_block_recoveries
                .with_label_values(&["submit_rejected"])
                .get()
                == 1
        })
        .await;
        // Still parked — not terminated.
        assert_eq!(
            h.context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            1
        );

        // The registered fetch later lands the block: sidecar delivered, quota freed.
        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| !h.service.lock().handle_excluded_ancestors.is_empty()).await;
        wait_until(|| {
            h.context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get()
                == 0
        })
        .await;
    }

    /// Per-peer admission is isolated and rolls back partial acquisitions; the charge
    /// covers payload, sidecar, task overhead, and frontier registrations.
    #[tokio::test]
    async fn quota_isolation_across_peers_and_rollback() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer_a = context.committee.to_authority_index(1).unwrap();
        let peer_b = context.committee.to_authority_index(2).unwrap();
        let quotas = RecoveryQuotas::with_limits(
            context.clone(),
            RecoveryLimits {
                peer_bytes: 10_000,
                peer_count: 2,
            },
        );

        // Charge arithmetic: payload + sidecar + overhead + per-slot registration.
        assert_eq!(admission_charge(1000, 100, 3), 1000 + 100 + 2048 + 3 * 256);

        // Saturate peer A's count lane; peer B is unaffected.
        let _a1 = quotas.try_acquire(peer_a, 4000).unwrap();
        let _a2 = quotas.try_acquire(peer_a, 4000).unwrap();
        assert_eq!(
            quotas.try_acquire(peer_a, 100).err(),
            Some(QuotaLimit::PeerCount)
        );
        let b1 = quotas.try_acquire(peer_b, 4000).unwrap();

        // Byte exhaustion on B: second acquisition of 7,000 exceeds 10,000 total.
        assert_eq!(
            quotas.try_acquire(peer_b, 7000).err(),
            Some(QuotaLimit::PeerBytes)
        );
        // The failed acquisition rolled back: 6,000 more still fits.
        let b2 = quotas.try_acquire(peer_b, 6000).unwrap();
        drop(b1);
        drop(b2);
        let b3 = quotas.try_acquire(peer_b, 10_000).unwrap();
        drop(b3);
    }
}
