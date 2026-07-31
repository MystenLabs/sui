// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Receive-side recovery for minimal blocks that cannot be inflated at receipt.
//!
//! Each un-inflatable minimal block becomes one bounded task that waits directly on
//! authoritative DAG state: when inflation reports a missing ancestor slot, the task
//! registers on `DagState`'s accepted-slot notifier, re-checks, and sleeps until the
//! slot fills or GC passes it. Inflation success against accepted-DAG candidates
//! implies the block's causal history is locally complete, so the inflated block is
//! submitted through the normal `handle_send_block` path immediately — each block
//! waits exactly once.
//!
//! Termination is guaranteed without deadlines: every waited slot either fills (the
//! network keeps moving, and a quorum of streams feeds acceptance) or falls below the
//! GC round (commits keep advancing), both of which wake the task. Ambiguity, digest
//! mismatch, or an exhausted retry budget escalate to a single digest-verified fetch
//! of the exact claimed block from its author.

use std::sync::{Arc, Weak};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::RwLock;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tracing::debug;

use crate::{
    block::{BlockAPI as _, SignedBlock, Slot, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    minimal_block::{FallbackReason, InflateError},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
};

/// Byte and count bounds on retained minimal payloads across all recovery tasks.
/// The global bytes cap bounds aggregate memory; the per-peer count cap prevents one
/// peer from consuming unbounded task overhead with tiny encodings.
const MAX_PARKED_BYTES: usize = 16 << 20;
const MAX_PARKED_BYTES_PER_PEER: usize = 2 << 20;
const MAX_PARKED_BLOCKS_PER_PEER: usize = 256;

/// Task-level inflation attempts (each preceded by a slot wait after the first)
/// before escalating to the exact-reference repair fetch. Distinct from the codec's
/// internal bounded candidate variations within one inflation call.
const MAX_RECOVERY_INFLATE_ATTEMPTS: usize = 3;

/// Concurrent exact-reference repair fetches, globally and per claimed author. Repair
/// is rare and Byzantine-driven; these caps keep one misbehaving peer from occupying
/// every repair slot.
const MAX_REPAIR_FETCHES: usize = 64;
const MAX_REPAIR_FETCHES_PER_PEER: usize = 6;
pub(crate) const RECOVERY_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Which quota bound rejected a recovery admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotaLimit {
    GlobalBytes,
    PeerBytes,
    PeerCount,
}

impl QuotaLimit {
    pub(crate) fn label(self) -> &'static str {
        match self {
            QuotaLimit::GlobalBytes => "global_bytes",
            QuotaLimit::PeerBytes => "peer_bytes",
            QuotaLimit::PeerCount => "peer_count",
        }
    }
}

/// Quota bounds, overridable in tests to force capacity pressure.
pub(crate) struct RecoveryLimits {
    pub(crate) global_bytes: usize,
    pub(crate) peer_bytes: usize,
    pub(crate) peer_count: usize,
}

impl Default for RecoveryLimits {
    fn default() -> Self {
        Self {
            global_bytes: MAX_PARKED_BYTES,
            peer_bytes: MAX_PARKED_BYTES_PER_PEER,
            peer_count: MAX_PARKED_BLOCKS_PER_PEER,
        }
    }
}

struct PeerQuota {
    bytes: Arc<Semaphore>,
    count: Arc<Semaphore>,
}

/// Admission control for recovery tasks. All acquisition is non-blocking and happens
/// before the task is spawned; the returned RAII permit releases every resource — and
/// corrects the parked gauges — on success, error, panic, task abort, and shutdown
/// alike.
pub(crate) struct RecoveryQuotas {
    context: Arc<Context>,
    peer_bytes_limit: usize,
    global_bytes: Arc<Semaphore>,
    per_peer: Vec<PeerQuota>,
}

impl RecoveryQuotas {
    pub(crate) fn new(context: Arc<Context>) -> Arc<Self> {
        Self::with_limits(context, RecoveryLimits::default())
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
            global_bytes: Arc::new(Semaphore::new(limits.global_bytes)),
            per_peer,
        })
    }

    pub(crate) fn try_acquire(
        self: &Arc<Self>,
        peer: AuthorityIndex,
        bytes: usize,
    ) -> Result<RecoveryPermit, QuotaLimit> {
        // A payload larger than the per-peer byte quota can never be admitted; report
        // it as the byte limit rather than deadlocking on an unsatisfiable acquire.
        if bytes > self.peer_bytes_limit {
            return Err(QuotaLimit::PeerBytes);
        }
        let bytes_u32 = u32::try_from(bytes).map_err(|_| QuotaLimit::PeerBytes)?;
        let quota = &self.per_peer[peer];
        let global = self
            .global_bytes
            .clone()
            .try_acquire_many_owned(bytes_u32)
            .map_err(|_| QuotaLimit::GlobalBytes)?;
        let peer_bytes = quota
            .bytes
            .clone()
            .try_acquire_many_owned(bytes_u32)
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
            .add(bytes as i64);
        Ok(RecoveryPermit {
            context: self.context.clone(),
            bytes,
            _global: global,
            _peer_bytes: peer_bytes,
            _peer_count: peer_count,
        })
    }
}

/// Held for the lifetime of one recovery task. The parked gauges mirror permit
/// existence exactly: incremented at acquisition, decremented on drop.
pub(crate) struct RecoveryPermit {
    context: Arc<Context>,
    bytes: usize,
    _global: OwnedSemaphorePermit,
    _peer_bytes: OwnedSemaphorePermit,
    _peer_count: OwnedSemaphorePermit,
}

impl Drop for RecoveryPermit {
    fn drop(&mut self) {
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.minimal_block_recovery_parked.dec();
        node_metrics
            .minimal_block_recovery_parked_bytes
            .sub(self.bytes as i64);
    }
}

/// Concurrency bounds for exact-reference repair fetches.
pub(crate) struct RepairLimits {
    global: Arc<Semaphore>,
    per_peer: Vec<Arc<Semaphore>>,
}

impl RepairLimits {
    pub(crate) fn new(committee_size: usize) -> Arc<Self> {
        Arc::new(Self {
            global: Arc::new(Semaphore::new(MAX_REPAIR_FETCHES)),
            per_peer: (0..committee_size)
                .map(|_| Arc::new(Semaphore::new(MAX_REPAIR_FETCHES_PER_PEER)))
                .collect(),
        })
    }
}

/// Terminal outcome labels for the `minimal_block_recoveries` metric.
enum RecoveryOutcome {
    Inflated,
    Repaired,
    Malformed,
    Obsolete,
    AlreadyAccepted,
    RepairFailed,
    SubmitRejected,
}

impl RecoveryOutcome {
    fn label(&self) -> &'static str {
        match self {
            RecoveryOutcome::Inflated => "inflated",
            RecoveryOutcome::Repaired => "repaired",
            RecoveryOutcome::Malformed => "malformed",
            RecoveryOutcome::Obsolete => "obsolete",
            RecoveryOutcome::AlreadyAccepted => "already_accepted",
            RecoveryOutcome::RepairFailed => "repair_failed",
            RecoveryOutcome::SubmitRejected => "submit_rejected",
        }
    }
}

enum RepairError {
    Fetch,
    Missing,
    Malformed,
    ReferenceMismatch,
}

impl RepairError {
    fn label(&self) -> &'static str {
        match self {
            RepairError::Fetch => "fetch_failed",
            RepairError::Missing => "missing",
            RepairError::Malformed => "malformed",
            RepairError::ReferenceMismatch => "reference_mismatch",
        }
    }
}

/// What the register-recheck under one DagState read guard concluded.
enum WaitCheck {
    /// The claimed block itself fell below GC: nothing to recover.
    Obsolete,
    /// The claimed block was accepted through another path (typically full-form
    /// replay after a reconnect): recovery is redundant, release the quota now.
    AlreadyAccepted,
    /// The waited slot fell below GC and can never fill: escalate to repair.
    Repair,
    /// A candidate is already accepted in the slot: retry inflation now.
    Retry,
    /// Slot still empty and above GC: await the registration.
    Wait,
}

/// Recovers one un-inflatable minimal block. Spawned (quota-first) by the subscriber;
/// aborted with its owning `JoinSet` on shutdown, which releases the permit and
/// deregisters any pending slot wait.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn recover_minimal_block<
    C: ValidatorNetworkClient,
    S: ValidatorNetworkService,
>(
    context: Arc<Context>,
    block_inflater: Arc<BlockInflater>,
    dag_state: Weak<RwLock<DagState>>,
    accepted_slots: Arc<NotifyRead<Slot, ()>>,
    mut gc_round: watch::Receiver<Round>,
    network_client: Arc<C>,
    authority_service: Weak<S>,
    repair_limits: Arc<RepairLimits>,
    peer: AuthorityIndex,
    claimed_ref: BlockRef,
    minimal: Bytes,
    permit: RecoveryPermit,
) {
    let _permit = permit;
    let parked_at = std::time::Instant::now();
    let node_metrics = &context.metrics.node_metrics;

    let mut outcome = RecoveryOutcome::RepairFailed;
    'recovery: {
        for attempt in 1..=MAX_RECOVERY_INFLATE_ATTEMPTS {
            let missing = match block_inflater.inflate(&minimal, peer) {
                Ok((_signed, serialized)) => {
                    // The wire saving is as real for a late inflation as for an
                    // immediate one; the receipt path only counts immediate successes.
                    let bytes_saved = serialized.len().saturating_sub(minimal.len()) as u64;
                    outcome = match submit(&context, &authority_service, peer, serialized).await {
                        Some(true) => {
                            node_metrics
                                .minimal_blocks_received_bytes_saved
                                .with_label_values(&[context
                                    .committee
                                    .authority(peer)
                                    .hostname
                                    .as_str()])
                                .inc_by(bytes_saved);
                            RecoveryOutcome::Inflated
                        }
                        Some(false) => RecoveryOutcome::SubmitRejected,
                        None => return,
                    };
                    break 'recovery;
                }
                Err(InflateError::NeedFullBlock {
                    reason: FallbackReason::MissingAncestor(slot),
                    ..
                }) => slot,
                Err(InflateError::NeedFullBlock { .. }) => {
                    // Ambiguity or a digest mismatch cannot be repaired by waiting.
                    outcome = repair(
                        &context,
                        network_client.as_ref(),
                        &authority_service,
                        &repair_limits,
                        peer,
                        claimed_ref,
                    )
                    .await;
                    break 'recovery;
                }
                Err(InflateError::Malformed(error)) => {
                    // Structurally malformed bytes cannot improve with later DAG state.
                    debug!(
                        "Dropping malformed minimal block {} from peer {}: {}",
                        claimed_ref, peer, error
                    );
                    outcome = RecoveryOutcome::Malformed;
                    break 'recovery;
                }
            };

            // The retry budget bounds slot waits; the next escalation is repair.
            if attempt == MAX_RECOVERY_INFLATE_ATTEMPTS {
                outcome = repair(
                    &context,
                    network_client.as_ref(),
                    &authority_service,
                    &repair_limits,
                    peer,
                    claimed_ref,
                )
                .await;
                break 'recovery;
            }

            // Register BEFORE re-checking the DAG: acceptance between the check and the
            // await has then already fired the registration, so no wake can be missed.
            // A second registration on the claimed block's own slot lets the task
            // terminate promptly when full-form replay (after a reconnect) delivers the
            // block through the normal path — the quota permit must not stay parked on
            // a wait the block no longer needs.
            loop {
                let mut registration = accepted_slots.register_one(&missing);
                let mut own_registration =
                    accepted_slots.register_one(&Slot::from(claimed_ref));
                let check = {
                    let Some(dag_state) = dag_state.upgrade() else {
                        return;
                    };
                    let dag_state = dag_state.read();
                    let gc_round = dag_state.gc_round();
                    if claimed_ref.round <= gc_round {
                        WaitCheck::Obsolete
                    } else if dag_state.contains_block(&claimed_ref) {
                        WaitCheck::AlreadyAccepted
                    } else if missing.round <= gc_round {
                        WaitCheck::Repair
                    } else if !dag_state.get_uncommitted_blocks_at_slot(missing).is_empty() {
                        WaitCheck::Retry
                    } else {
                        WaitCheck::Wait
                    }
                };
                match check {
                    WaitCheck::Obsolete => {
                        outcome = RecoveryOutcome::Obsolete;
                        break 'recovery;
                    }
                    WaitCheck::AlreadyAccepted => {
                        outcome = RecoveryOutcome::AlreadyAccepted;
                        break 'recovery;
                    }
                    WaitCheck::Repair => {
                        outcome = repair(
                            &context,
                            network_client.as_ref(),
                            &authority_service,
                            &repair_limits,
                            peer,
                            claimed_ref,
                        )
                        .await;
                        break 'recovery;
                    }
                    WaitCheck::Retry => break,
                    WaitCheck::Wait => {}
                }
                tokio::select! {
                    _ = &mut registration => break,
                    // A wake on the own slot may be an equivocating sibling: loop and
                    // re-check rather than assuming the claimed block landed.
                    _ = &mut own_registration => continue,
                    changed = gc_round.changed() => {
                        if changed.is_err() {
                            // DagState (and its watch sender) is gone: shutdown.
                            return;
                        }
                        // Re-evaluate the slot against the advanced GC round.
                        continue;
                    }
                }
            }
        }
    }

    match outcome {
        RecoveryOutcome::Inflated | RecoveryOutcome::Repaired => {
            node_metrics
                .minimal_block_recovery_latency
                .observe(parked_at.elapsed().as_secs_f64());
        }
        _ => {}
    }
    node_metrics
        .minimal_block_recoveries
        .with_label_values(&[outcome.label()])
        .inc();
}

/// Submits recovered full-form bytes through the normal receive path, deliberately
/// reusing its parsing, signature verification, vote tracking, and Core dispatch.
/// Returns None on shutdown, Some(accepted) otherwise.
async fn submit<S: ValidatorNetworkService>(
    context: &Context,
    authority_service: &Weak<S>,
    peer: AuthorityIndex,
    serialized: Bytes,
) -> Option<bool> {
    let Some(authority_service) = authority_service.upgrade() else {
        return None;
    };
    let block = ExtendedSerializedBlock {
        block: serialized,
        minimal: None,
        excluded_ancestors: vec![],
    };
    match authority_service.handle_send_block(peer, block).await {
        Ok(()) => Some(true),
        Err(error) => {
            debug!(
                "Recovered minimal block from peer {} {} was rejected: {}",
                peer,
                context.committee.authority(peer).hostname,
                error
            );
            Some(false)
        }
    }
}

/// One bounded, digest-verified fetch of the exact claimed block from its author.
/// Other peers are deliberately not tried: the sending peer authored the block and
/// should possess it; later full descendants and normal missing-ancestor sync remain
/// the backstop when the author fails to serve it.
async fn repair<C: ValidatorNetworkClient, S: ValidatorNetworkService>(
    context: &Context,
    network_client: &C,
    authority_service: &Weak<S>,
    repair_limits: &RepairLimits,
    peer: AuthorityIndex,
    claimed_ref: BlockRef,
) -> RecoveryOutcome {
    let node_metrics = &context.metrics.node_metrics;
    // Bounded waits: tasks queued here hold RecoveryPermits, so the queue is bounded
    // by the parked-block quotas. acquire fails only on semaphore closure.
    let (Ok(_global), Ok(_per_peer)) = (
        repair_limits.global.clone().acquire_owned().await,
        repair_limits.per_peer[peer].clone().acquire_owned().await,
    ) else {
        return RecoveryOutcome::RepairFailed;
    };
    match fetch_exact_block(network_client, peer, claimed_ref).await {
        Ok(serialized) => match submit(context, authority_service, peer, serialized).await {
            Some(true) => {
                node_metrics
                    .minimal_block_repairs
                    .with_label_values(&["success"])
                    .inc();
                RecoveryOutcome::Repaired
            }
            Some(false) => {
                node_metrics
                    .minimal_block_repairs
                    .with_label_values(&["submit_rejected"])
                    .inc();
                RecoveryOutcome::SubmitRejected
            }
            None => RecoveryOutcome::RepairFailed,
        },
        Err(error) => {
            node_metrics
                .minimal_block_repairs
                .with_label_values(&[error.label()])
                .inc();
            RecoveryOutcome::RepairFailed
        }
    }
}

/// Fetches `block_ref` from `peer` and returns the response bytes whose computed
/// reference matches it exactly. Unrelated response entries are ignored; a response
/// without an exact match fails the repair.
async fn fetch_exact_block<C: ValidatorNetworkClient>(
    network_client: &C,
    peer: AuthorityIndex,
    block_ref: BlockRef,
) -> Result<Bytes, RepairError> {
    let blocks = network_client
        .fetch_blocks(peer, vec![block_ref], vec![], false, RECOVERY_FETCH_TIMEOUT)
        .await
        .map_err(|error| {
            debug!(
                "Repair fetch of {} from peer {} failed: {}",
                block_ref, peer, error
            );
            RepairError::Fetch
        })?;
    if blocks.is_empty() {
        return Err(RepairError::Missing);
    }
    let mut saw_parseable = false;
    for bytes in blocks {
        let Ok(signed) = bcs::from_bytes::<SignedBlock>(&bytes) else {
            continue;
        };
        saw_parseable = true;
        let reference = BlockRef::new(
            signed.round(),
            signed.author(),
            VerifiedBlock::compute_digest(&bytes),
        );
        if reference == block_ref {
            return Ok(bytes);
        }
    }
    if saw_parseable {
        // The author could not produce its own claimed block: a peer fault.
        Err(RepairError::ReferenceMismatch)
    } else {
        Err(RepairError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use consensus_types::block::BlockDigest;
    use parking_lot::Mutex;

    use super::*;
    use crate::{
        VerifiedBlock,
        block::TestBlock,
        commit::{CommitDigest, CommitRange, TrustedCommit},
        network::{BlockStream, test_network::TestService},
        storage::mem_store::MemStore,
    };

    /// Serves repair fetches from a fixed set; everything else is unreachable in
    /// these tests.
    struct RepairOnlyClient {
        fetchable: Vec<Bytes>,
        fetch_calls: Mutex<Vec<Vec<BlockRef>>>,
    }

    impl RepairOnlyClient {
        fn new(fetchable: Vec<Bytes>) -> Self {
            Self {
                fetchable,
                fetch_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ValidatorNetworkClient for RepairOnlyClient {
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _block: &VerifiedBlock,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }

        async fn subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<BlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<Vec<Bytes>> {
            self.fetch_calls.lock().push(block_refs);
            Ok(self.fetchable.clone())
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_latest_blocks(
            &self,
            _peer: AuthorityIndex,
            _authorities: Vec<AuthorityIndex>,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn get_latest_rounds(
            &self,
            _peer: AuthorityIndex,
            _timeout: Duration,
        ) -> crate::error::ConsensusResult<(Vec<Round>, Vec<Round>)> {
            unimplemented!("Unimplemented")
        }
    }

    /// One recovery-task scenario: a receiver DAG, a peer block whose minimal form
    /// misses `missing_count` ancestors, and everything needed to spawn the task.
    struct TaskHarness {
        context: Arc<Context>,
        peer: AuthorityIndex,
        receiver_dag: Arc<RwLock<DagState>>,
        inflater: Arc<BlockInflater>,
        service: Arc<Mutex<TestService>>,
        quotas: Arc<RecoveryQuotas>,
        repair_limits: Arc<RepairLimits>,
        // Ancestors of `block`, NOT accepted into the receiver DAG.
        ancestors: Vec<VerifiedBlock>,
        block: VerifiedBlock,
        minimal: Bytes,
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
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(11, peer.value() as u32)
                .set_ancestors_raw(ancestor_refs)
                .build(),
        );
        let minimal = sender_inflater.serialize(&block).unwrap();

        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let inflater = Arc::new(BlockInflater::new(context.clone(), receiver_dag.clone()));
        TaskHarness {
            peer,
            receiver_dag,
            inflater,
            service: Arc::new(Mutex::new(TestService::new())),
            quotas: RecoveryQuotas::new(context.clone()),
            repair_limits: RepairLimits::new(4),
            context,
            ancestors,
            block,
            minimal,
        }
    }

    impl TaskHarness {
        fn spawn(
            &self,
            join_set: &mut tokio::task::JoinSet<()>,
            client: Arc<RepairOnlyClient>,
        ) {
            let permit = self
                .quotas
                .try_acquire(self.peer, self.minimal.len())
                .unwrap();
            let (accepted_slots, gc_round) = {
                let dag_state = self.receiver_dag.read();
                (
                    dag_state.accepted_slot_notifier(),
                    dag_state.gc_round_receiver(),
                )
            };
            join_set.spawn(recover_minimal_block(
                self.context.clone(),
                self.inflater.clone(),
                Arc::downgrade(&self.receiver_dag),
                accepted_slots,
                gc_round,
                client,
                Arc::downgrade(&self.service),
                self.repair_limits.clone(),
                self.peer,
                self.block.reference(),
                self.minimal.clone(),
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

    /// The register-then-recheck ordering must close the race at every boundary:
    /// acceptance strictly before the task starts, and acceptance racing the task's
    /// first wait, must both complete the recovery without hanging.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_closes_register_then_accept_race() {
        // Acceptance BEFORE the task starts: the recheck (not the wake) must find it.
        {
            let h = harness(None);
            let client = Arc::new(RepairOnlyClient::new(vec![]));
            h.receiver_dag.write().accept_blocks(h.ancestors.clone());
            let mut join_set = tokio::task::JoinSet::new();
            h.spawn(&mut join_set, client.clone());
            wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
            assert!(client.fetch_calls.lock().is_empty());
        }
        // Acceptance racing the parked task, at a few interleaving offsets.
        for delay_ms in [0u64, 1, 5, 25] {
            let h = harness(None);
            let client = Arc::new(RepairOnlyClient::new(vec![]));
            let mut join_set = tokio::task::JoinSet::new();
            h.spawn(&mut join_set, client.clone());
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            h.receiver_dag.write().accept_blocks(h.ancestors.clone());
            wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
            let received = h.service.lock().handle_send_block.clone();
            assert_eq!(&received[0].1.block, h.block.serialized(), "{delay_ms}ms");
            assert!(client.fetch_calls.lock().is_empty(), "{delay_ms}ms");
        }
    }

    /// A block missing several ancestors re-keys from slot to slot as each lands,
    /// finally submitting once — with zero fetches.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_rekeys_across_missing_slots() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // Ancestor order in the minimal encoding is own-author-first: land the first
        // missing slot alone, then the rest in one batch. The task re-keys from the
        // first slot to the next and completes within its attempt budget.
        let own_first = h
            .ancestors
            .iter()
            .find(|b| b.author() == h.peer)
            .unwrap()
            .clone();
        h.receiver_dag.write().accept_block(own_first.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        let rest: Vec<_> = h
            .ancestors
            .iter()
            .filter(|b| b.author() != h.peer)
            .cloned()
            .collect();
        h.receiver_dag.write().accept_blocks(rest);
        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, h.block.serialized());
        assert!(client.fetch_calls.lock().is_empty());
        assert_eq!(
            h.context
                .metrics
                .node_metrics
                .minimal_block_recoveries
                .with_label_values(&["inflated"])
                .get(),
            1
        );
    }

    /// Equivocating candidates within the codec budget still inflate the claimed
    /// reconstruction when the true ancestor is among the accepted candidates.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_inflates_with_equivocation_candidates() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        // Both the true ancestors and an equivocating sibling (same slot, different
        // digest) are accepted before the task runs. The sibling's author must not be
        // the receiver's own index: DagState asserts against own-slot equivocation.
        h.receiver_dag.write().accept_blocks(h.ancestors.clone());
        let sibling = VerifiedBlock::new_for_test(
            TestBlock::new(10, 1).set_timestamp_ms(999_999).build(),
        );
        h.receiver_dag.write().accept_block(sibling);
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(&received[0].1.block, h.block.serialized());
        assert!(client.fetch_calls.lock().is_empty());
    }

    /// A wake by the WRONG sibling in the waited slot must never submit a wrong
    /// reconstruction: the digest gate fails it and exact repair fetches the claimed
    /// block only.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_repairs_when_wrong_sibling_arrives_first() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![h.block.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Fill every missing slot with a WRONG sibling (equivocations with digests the
        // block does not reference).
        let siblings: Vec<_> = h
            .ancestors
            .iter()
            .map(|b| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, b.author().value() as u32)
                        .set_timestamp_ms(999_999)
                        .build(),
                )
            })
            .collect();
        h.receiver_dag.write().accept_blocks(siblings);

        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        // Only the exact claimed block was submitted, and only via the repair fetch.
        assert_eq!(&received[0].1.block, h.block.serialized());
        assert_eq!(
            client.fetch_calls.lock().as_slice(),
            &[vec![h.block.reference()]]
        );
        assert_eq!(
            h.context
                .metrics
                .node_metrics
                .minimal_block_repairs
                .with_label_values(&["success"])
                .get(),
            1
        );
    }

    /// Exhausting the inflate-attempt budget escalates to exactly one repair fetch.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_repairs_after_attempt_budget_exhausted() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![h.block.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());

        // Feed missing ancestors ONE at a time: each acceptance wakes the task into
        // another inflation that discovers the next missing slot. Attempt 3 comes up
        // still-missing, so the task escalates to repair rather than waiting again.
        for ancestor in h.ancestors.iter().filter(|b| b.author() != h.peer).take(2) {
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert!(h.service.lock().handle_send_block.is_empty());
            h.receiver_dag.write().accept_block(ancestor.clone());
        }
        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        assert_eq!(
            client.fetch_calls.lock().as_slice(),
            &[vec![h.block.reference()]]
        );
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(&received[0].1.block, h.block.serialized());
    }

    /// GC crossing the WAITED slot ends the wait and repairs the child instead.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_gc_cross_triggers_repair() {
        let h = harness(Some(3));
        let client = Arc::new(RepairOnlyClient::new(vec![h.block.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // Commit with leader round 13: gc_round = 10 >= the waited slot (10), while
        // the claimed block (11) stays above GC.
        let leader = BlockRef::new(13, h.peer, BlockDigest::MIN);
        let commit = TrustedCommit::new_for_test(1, CommitDigest::MIN, 100, leader, vec![leader]);
        h.receiver_dag.write().add_commit(commit);

        wait_until(|| !h.service.lock().handle_send_block.is_empty()).await;
        assert_eq!(
            client.fetch_calls.lock().as_slice(),
            &[vec![h.block.reference()]]
        );
        let received = h.service.lock().handle_send_block.clone();
        assert_eq!(&received[0].1.block, h.block.serialized());
    }

    /// GC crossing the CLAIMED block itself makes recovery pointless: no fetch, no
    /// submission, outcome obsolete.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_drops_child_when_child_itself_is_gced() {
        let h = harness(Some(3));
        let client = Arc::new(RepairOnlyClient::new(vec![h.block.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Commit with leader round 20: gc_round = 17 >= the claimed block round (11).
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
        assert!(client.fetch_calls.lock().is_empty());
    }

    /// Aborting the owning JoinSet mid-wait releases the quota permit, corrects the
    /// gauges, and deregisters the slot wait.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_shutdown_aborts_waiters_and_releases_quota() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;

        let node_metrics = &h.context.metrics.node_metrics;
        let notifier = h.receiver_dag.read().accepted_slot_notifier();
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);
        // Two registrations per waiter: the missing slot and the claimed block's own
        // slot (the replay-termination wake).
        assert_eq!(notifier.num_pending(), 2);

        // shutdown() aborts and awaits every task: cleanup is deterministic.
        join_set.shutdown().await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 0);
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
        assert_eq!(notifier.num_pending(), 0);
        // The permit is immediately reusable.
        let permit = h.quotas.try_acquire(h.peer, h.minimal.len()).unwrap();
        drop(permit);
    }

    /// When the claimed block itself lands through another path (full-form replay
    /// after a reconnect), the parked task must terminate and release its quota — not
    /// keep waiting on an ancestor slot the block no longer needs. A sibling in the
    /// own slot must NOT trigger this.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_terminates_when_claimed_block_arrives_via_replay() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        let node_metrics = &h.context.metrics.node_metrics;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);

        // A sibling in the claimed block's own slot wakes the task but must not
        // terminate it: the claimed block itself is still unaccepted.
        let sibling = VerifiedBlock::new_for_test(
            TestBlock::new(11, h.peer.value() as u32)
                .set_timestamp_ms(777_777)
                .build(),
        );
        h.receiver_dag.write().accept_block(sibling);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);

        // The claimed block arrives (as replay would deliver it): the task ends,
        // submits nothing, fetches nothing, and frees its permit.
        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert_eq!(
            node_metrics
                .minimal_block_recoveries
                .with_label_values(&["already_accepted"])
                .get(),
            1
        );
        assert!(h.service.lock().handle_send_block.is_empty());
        assert!(client.fetch_calls.lock().is_empty());
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
    }

    /// A repair response whose computed reference differs from the claimed one is a
    /// peer fault: nothing may be submitted.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn exact_repair_rejects_wrong_digest_response() {
        let h = harness(None);
        // The author serves a DIFFERENT block than the claimed one.
        let wrong = VerifiedBlock::new_for_test(
            TestBlock::new(11, h.peer.value() as u32)
                .set_timestamp_ms(424_242)
                .build(),
        );
        let client = Arc::new(RepairOnlyClient::new(vec![wrong.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Wrong siblings force the digest-mismatch path into repair.
        let siblings: Vec<_> = h
            .ancestors
            .iter()
            .map(|b| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, b.author().value() as u32)
                        .set_timestamp_ms(999_999)
                        .build(),
                )
            })
            .collect();
        h.receiver_dag.write().accept_blocks(siblings);

        wait_until(|| {
            h.context
                .metrics
                .node_metrics
                .minimal_block_repairs
                .with_label_values(&["reference_mismatch"])
                .get()
                == 1
        })
        .await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert_eq!(
            h.context
                .metrics
                .node_metrics
                .minimal_block_recoveries
                .with_label_values(&["repair_failed"])
                .get(),
            1
        );
    }
}
