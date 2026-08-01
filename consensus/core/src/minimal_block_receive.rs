// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Receive-side recovery for minimal blocks that cannot be inflated at receipt.
//!
//! Each un-inflatable minimal block becomes one bounded task that waits directly on
//! authoritative DAG state: inflation reports the COMPLETE frontier of missing
//! ancestor slots, the task registers on all of them via `DagState`'s accepted-slot
//! notifier, re-checks, and sleeps until the whole frontier fills — one wake, one
//! re-inflation. Inflation success against accepted-DAG candidates implies the
//! block's causal history is locally complete, so the inflated block is submitted
//! through the normal `handle_send_block` path immediately.
//!
//! Termination is guaranteed without deadlines: every waited slot either fills (the
//! network keeps moving, and a quorum of streams feeds acceptance) or falls below the
//! GC round (commits keep advancing), both of which wake the task. Ambiguity, a
//! digest mismatch, or a required slot crossing GC under a live child escalate to a
//! single digest-verified fetch of the exact claimed block from its author; an
//! exhausted re-inflation budget (exceptional once the frontier is complete) DROPS
//! the block — descendants that do inflate drive normal missing-ancestor sync, which
//! is the backstop. Ordinary tip racing must never produce fetches.

use std::sync::{Arc, Weak};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::BlockRef;
use itertools::Itertools as _;
use mysten_common::debug_fatal;
use parking_lot::RwLock;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{debug, warn};

use crate::{
    block::{BlockAPI as _, SignedBlock, Slot, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    minimal_block::{FallbackReason, InflateError},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
};

/// Bounds on recovery admission. Parking is NORMAL at tip: at a large committee a
/// sizable fraction of live minimal blocks race their ancestors' arrival (43%
/// measured at 132 validators) and park for around a second, so the quotas must fit
/// steady-state racing with generous tail headroom — the v3 run showed that a quota
/// sized for the average (16 MiB ≈ a few thousand blocks) saturates on the residency
/// tail and turns ordinary racing into reset/full-replay storms. 64 MiB gives ~4x
/// headroom over the worst node observed in that degraded run; the global count cap
/// bounds task overhead when payloads are tiny.
///
/// The per-peer byte cap is derived from the subscriber's pre-decode size cap at
/// construction (never below it): a legitimate minimal block larger than the peer
/// quota could never be admitted, and every arrival would divert to a stream reset.
///
/// Waiter slots are their own resource: a frontier wait registers one notifier entry
/// per missing slot, so admission charges the frontier size — otherwise one peer's
/// parked payload bytes could pin hundreds of thousands of registrations.
const MAX_PARKED_BYTES: usize = 64 << 20;
const MAX_PARKED_BLOCKS: usize = 32_768;
const MIN_PARKED_BYTES_PER_PEER: usize = 4 << 20;
const MAX_PARKED_BLOCKS_PER_PEER: usize = 256;
const MAX_WAITER_SLOTS: usize = 1 << 20;

/// Cap on untrusted minimal bytes, enforced before ANY decoding: a legitimate
/// minimal block is bounded by the max transaction payload plus small per-ancestor
/// structure.
pub(crate) fn max_minimal_size(context: &Context) -> usize {
    (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2)
}

/// Task-level inflation attempts (each preceded by a slot wait after the first)
/// before escalating to the exact-reference repair fetch. Distinct from the codec's
/// internal bounded candidate variations within one inflation call.
const MAX_RECOVERY_INFLATE_ATTEMPTS: usize = 3;

/// Concurrent exact-reference repair fetches, globally and per claimed author. Repair
/// is rare and Byzantine-driven; these caps keep one misbehaving peer from occupying
/// every repair slot.
const MAX_REPAIR_FETCHES: usize = 64;
const MAX_REPAIR_FETCHES_PER_PEER: usize = 6;
const RECOVERY_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Which quota bound rejected a recovery admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotaLimit {
    GlobalBytes,
    GlobalCount,
    PeerBytes,
    PeerCount,
    WaiterSlots,
}

impl QuotaLimit {
    pub(crate) fn label(self) -> &'static str {
        match self {
            QuotaLimit::GlobalBytes => "global_bytes",
            QuotaLimit::GlobalCount => "global_count",
            QuotaLimit::PeerBytes => "peer_bytes",
            QuotaLimit::PeerCount => "peer_count",
            QuotaLimit::WaiterSlots => "waiter_slots",
        }
    }
}

/// Quota bounds, overridable in tests to force capacity pressure.
pub(crate) struct RecoveryLimits {
    pub(crate) global_bytes: usize,
    pub(crate) global_count: usize,
    pub(crate) peer_bytes: usize,
    pub(crate) peer_count: usize,
    pub(crate) waiter_slots: usize,
}

impl Default for RecoveryLimits {
    fn default() -> Self {
        Self {
            global_bytes: MAX_PARKED_BYTES,
            global_count: MAX_PARKED_BLOCKS,
            peer_bytes: MIN_PARKED_BYTES_PER_PEER,
            peer_count: MAX_PARKED_BLOCKS_PER_PEER,
            waiter_slots: MAX_WAITER_SLOTS,
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
    global_count: Arc<Semaphore>,
    waiter_slots: Arc<Semaphore>,
    per_peer: Vec<PeerQuota>,
}

impl RecoveryQuotas {
    pub(crate) fn new(context: Arc<Context>) -> Arc<Self> {
        // The per-peer byte quota must always admit a maximum-size legitimate
        // minimal block, whatever the protocol's size cap becomes.
        let limits = RecoveryLimits {
            peer_bytes: MIN_PARKED_BYTES_PER_PEER.max(max_minimal_size(&context)),
            ..RecoveryLimits::default()
        };
        Self::with_limits(context, limits)
    }

    pub(crate) fn with_limits(context: Arc<Context>, limits: RecoveryLimits) -> Arc<Self> {
        let peer_bytes_limit = limits.peer_bytes;
        let per_peer = (0..context.committee.size())
            .map(|_| PeerQuota {
                bytes: Arc::new(Semaphore::new(peer_bytes_limit)),
                count: Arc::new(Semaphore::new(limits.peer_count)),
            })
            .collect();
        Arc::new(Self {
            context,
            peer_bytes_limit,
            global_bytes: Arc::new(Semaphore::new(limits.global_bytes)),
            global_count: Arc::new(Semaphore::new(limits.global_count)),
            waiter_slots: Arc::new(Semaphore::new(limits.waiter_slots)),
            per_peer,
        })
    }

    pub(crate) fn try_acquire(
        self: &Arc<Self>,
        peer: AuthorityIndex,
        bytes: usize,
        wait_slots: usize,
    ) -> Result<RecoveryPermit, QuotaLimit> {
        // A payload larger than the per-peer byte quota can never be admitted; report
        // it as the byte limit rather than deadlocking on an unsatisfiable acquire.
        if bytes > self.peer_bytes_limit {
            return Err(QuotaLimit::PeerBytes);
        }
        let bytes_u32 = u32::try_from(bytes).map_err(|_| QuotaLimit::PeerBytes)?;
        let slots_u32 = u32::try_from(wait_slots).map_err(|_| QuotaLimit::WaiterSlots)?;
        let quota = &self.per_peer[peer];
        let global = self
            .global_bytes
            .clone()
            .try_acquire_many_owned(bytes_u32)
            .map_err(|_| QuotaLimit::GlobalBytes)?;
        let global_count = self
            .global_count
            .clone()
            .try_acquire_owned()
            .map_err(|_| QuotaLimit::GlobalCount)?;
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
        let waiters = self
            .waiter_slots
            .clone()
            .try_acquire_many_owned(slots_u32)
            .map_err(|_| QuotaLimit::WaiterSlots)?;
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.minimal_block_recovery_parked.inc();
        node_metrics
            .minimal_block_recovery_parked_bytes
            .add(bytes as i64);
        node_metrics
            .minimal_block_recovery_waiters
            .add(wait_slots as i64);
        Ok(RecoveryPermit {
            context: self.context.clone(),
            bytes,
            wait_slots,
            _global: global,
            _global_count: global_count,
            _peer_bytes: peer_bytes,
            _peer_count: peer_count,
            _waiters: waiters,
        })
    }
}

/// Held for the lifetime of one recovery task. The parked gauges mirror permit
/// existence exactly: incremented at acquisition, decremented on drop.
pub(crate) struct RecoveryPermit {
    context: Arc<Context>,
    bytes: usize,
    wait_slots: usize,
    _global: OwnedSemaphorePermit,
    _global_count: OwnedSemaphorePermit,
    _peer_bytes: OwnedSemaphorePermit,
    _peer_count: OwnedSemaphorePermit,
    _waiters: OwnedSemaphorePermit,
}

impl Drop for RecoveryPermit {
    fn drop(&mut self) {
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.minimal_block_recovery_parked.dec();
        node_metrics
            .minimal_block_recovery_parked_bytes
            .sub(self.bytes as i64);
        node_metrics
            .minimal_block_recovery_waiters
            .sub(self.wait_slots as i64);
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
    Obsolete,
    AlreadyAccepted,
    RetryExhausted,
    RepairFailed,
    SubmitRejected,
}

impl RecoveryOutcome {
    fn label(&self) -> &'static str {
        match self {
            RecoveryOutcome::Inflated => "inflated",
            RecoveryOutcome::Repaired => "repaired",
            RecoveryOutcome::Obsolete => "obsolete",
            RecoveryOutcome::AlreadyAccepted => "already_accepted",
            RecoveryOutcome::RetryExhausted => "retry_exhausted",
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
enum WaitCheck<'a> {
    /// The claimed block itself fell below GC: nothing to recover.
    Obsolete,
    /// The claimed block was accepted through another path (typically full-form
    /// replay after a reconnect): recovery is redundant, release the quota now.
    AlreadyAccepted,
    /// A required slot fell below GC while the child is live: without its omitted
    /// digest the minimal can never inflate — only the exact full block resolves it.
    Repair,
    /// Every slot in the frontier has an accepted candidate: retry inflation now.
    Retry,
    /// Await the registrations of the still-empty slots as one barrier.
    Wait(Vec<mysten_common::sync::notify_read::Registration<'a, Slot, ()>>),
}

/// Recovers one un-inflatable minimal block, starting from the receipt-time
/// classification (`reason`) so the same immutable bytes are not re-decoded before the
/// first wait. Spawned (quota-first) by the subscriber; aborted with its owning
/// `JoinSet` on shutdown, which releases the permit and deregisters any pending wait.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn recover_minimal_block<C: ValidatorNetworkClient, S: ValidatorNetworkService>(
    context: Arc<Context>,
    block_inflater: Arc<BlockInflater>,
    dag_state: Weak<RwLock<DagState>>,
    network_client: Arc<C>,
    authority_service: Weak<S>,
    repair_limits: Arc<RepairLimits>,
    peer: AuthorityIndex,
    claimed_ref: BlockRef,
    reason: FallbackReason,
    minimal: Bytes,
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

    let mut reason = reason;
    // The receipt-time inflation that produced `reason` was attempt one; the budget
    // counts inflations of these immutable bytes, wherever they run. With the codec
    // reporting the COMPLETE missing frontier, the second attempt normally succeeds;
    // exhaustion is exceptional and terminates as a drop, never a fetch — normal
    // missing-ancestor sync (driven by descendants that do inflate) is the backstop.
    let mut inflate_attempts = 1;
    let outcome = 'recovery: {
        loop {
            let missing = match reason {
                FallbackReason::MissingAncestors(slots) => slots,
                // Ambiguity or a digest mismatch cannot be repaired by waiting.
                FallbackReason::AmbiguousSlot(_) | FallbackReason::DigestMismatch => {
                    break 'recovery repair(
                        &context,
                        network_client.as_ref(),
                        &authority_service,
                        &repair_limits,
                        peer,
                        claimed_ref,
                    )
                    .await;
                }
            };
            node_metrics
                .minimal_block_park_missing_slots
                .observe(missing.len() as f64);

            // Register on EVERY missing slot before re-checking the DAG, so an
            // acceptance between the check and the await has already fired its
            // registration — then wait for the whole frontier as one barrier and
            // re-inflate once, instead of rediscovering the frontier one wake at a
            // time. A registration on the claimed block's exact reference lets the
            // task terminate promptly when full-form replay delivers the block
            // through the normal path (keyed by BlockRef, not slot: equivocating
            // siblings must not wake every parked task for the slot).
            'wait: loop {
                let registrations = accepted_slots.register_all(&missing);
                let mut own_registration = accepted_refs.register_one(&claimed_ref);
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
                    } else {
                        let mut remaining = Vec::new();
                        let mut crossed_gc = false;
                        for (slot, registration) in missing.iter().zip_eq(registrations) {
                            if !dag_state.get_uncommitted_blocks_at_slot(*slot).is_empty() {
                                // Already filled: dropping deregisters it.
                            } else if slot.round <= gc_round {
                                // This slot can never fill, and without its omitted
                                // digest the minimal can never inflate: only the
                                // exact full block can resolve the live child.
                                crossed_gc = true;
                            } else {
                                remaining.push(registration);
                            }
                        }
                        if crossed_gc {
                            WaitCheck::Repair
                        } else if remaining.is_empty() {
                            WaitCheck::Retry
                        } else {
                            WaitCheck::Wait(remaining)
                        }
                    }
                };
                match check {
                    WaitCheck::Obsolete => break 'recovery RecoveryOutcome::Obsolete,
                    WaitCheck::AlreadyAccepted => {
                        break 'recovery RecoveryOutcome::AlreadyAccepted;
                    }
                    WaitCheck::Repair => {
                        break 'recovery repair(
                            &context,
                            network_client.as_ref(),
                            &authority_service,
                            &repair_limits,
                            peer,
                            claimed_ref,
                        )
                        .await;
                    }
                    WaitCheck::Retry => break 'wait,
                    WaitCheck::Wait(remaining) => {
                        let frontier = futures::future::join_all(remaining);
                        tokio::pin!(frontier);
                        tokio::select! {
                            _ = &mut frontier => break 'wait,
                            // The exact claimed block was accepted; loop into the
                            // re-check, which concludes AlreadyAccepted.
                            _ = &mut own_registration => continue 'wait,
                            changed = gc_round.changed() => {
                                if changed.is_err() {
                                    // DagState (and its watch sender) is gone.
                                    return;
                                }
                                // Re-register and re-evaluate every slot against
                                // the advanced GC round.
                                continue 'wait;
                            }
                        }
                    }
                }
            }

            // The whole frontier has candidates: re-classify by inflating.
            if inflate_attempts == MAX_RECOVERY_INFLATE_ATTEMPTS {
                warn!(
                    "Recovery of minimal block {} from peer {} exhausted {} inflation \
                     attempts; dropping (sync recovers via descendants)",
                    claimed_ref, peer, inflate_attempts
                );
                break 'recovery RecoveryOutcome::RetryExhausted;
            }
            inflate_attempts += 1;
            let inflated = {
                let Some(dag_state) = dag_state.upgrade() else {
                    return;
                };
                let dag_state = dag_state.read();
                block_inflater.inflate(&minimal, peer, &dag_state)
            };
            match inflated {
                Ok((_signed, serialized)) => {
                    break 'recovery match submit(&context, &authority_service, peer, serialized)
                        .await
                    {
                        Some(true) => RecoveryOutcome::Inflated,
                        Some(false) => RecoveryOutcome::SubmitRejected,
                        None => return,
                    };
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
    let authority_service = authority_service.upgrade()?;
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
    // by the parked-block quotas; acquire fails only on semaphore closure. The
    // per-peer permit is taken FIRST so one peer with a saturated repair lane can
    // occupy at most MAX_REPAIR_FETCHES_PER_PEER global slots — global-first would
    // let it park quota-bounded task counts on the global semaphore and starve every
    // other peer's repairs.
    let (Ok(_per_peer), Ok(_global)) = (
        repair_limits.per_peer[peer].clone().acquire_owned().await,
        repair_limits.global.clone().acquire_owned().await,
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

    use consensus_types::block::Round;

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
            repair_limits: RepairLimits::new(4),
            context,
            ancestors,
            block,
            minimal,
            reason,
        }
    }

    impl TaskHarness {
        fn spawn(&self, join_set: &mut tokio::task::JoinSet<()>, client: Arc<RepairOnlyClient>) {
            let wait_slots = match &self.reason {
                FallbackReason::MissingAncestors(slots) => slots.len(),
                _ => 0,
            };
            let permit = self
                .quotas
                .try_acquire(self.peer, self.minimal.len(), wait_slots)
                .unwrap();
            join_set.spawn(recover_minimal_block(
                self.context.clone(),
                self.inflater.clone(),
                Arc::downgrade(&self.receiver_dag),
                client,
                Arc::downgrade(&self.service),
                self.repair_limits.clone(),
                self.peer,
                self.block.reference(),
                self.reason.clone(),
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

    /// The task waits on the whole missing frontier as one barrier: partial arrivals
    /// neither re-inflate nor submit, and the final arrival produces exactly one
    /// submission — with zero fetches.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_waits_for_full_frontier_then_inflates_once() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // Fill the frontier one slot at a time (the own parent rides an explicit
        // digest and is not part of it): nothing may submit before the last one.
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
    }

    /// GC crossing ONE of several awaited frontier slots ends the wait and repairs
    /// the live child: that slot can never fill, and without its omitted digest the
    /// minimal can never inflate — waiting on the others would be pointless.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_gc_cross_of_one_frontier_slot_triggers_repair() {
        let h = harness(Some(3));
        let client = Arc::new(RepairOnlyClient::new(vec![h.block.serialized().clone()]));
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // Fill two of the three frontier slots; the third stays empty.
        let filled: Vec<_> = h
            .ancestors
            .iter()
            .filter(|b| b.author() != h.peer)
            .take(2)
            .cloned()
            .collect();
        h.receiver_dag.write().accept_blocks(filled);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(h.service.lock().handle_send_block.is_empty());

        // Commit with leader round 13: gc_round = 10 >= the unfilled slot (10),
        // while the claimed block (11) stays above GC.
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
    /// submission, outcome obsolete. The single commit here crosses the whole missing
    /// frontier (round 10) AND the claimed child (round 11) together, so this also
    /// pins the precedence: Obsolete wins over the frontier's GC-cross repair.
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
        let slot_notifier = h.receiver_dag.read().accepted_slot_notifier();
        let ref_notifier = h.receiver_dag.read().accepted_ref_notifier();
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);
        // One registration per missing-frontier slot (three non-own ancestors; the
        // own parent rides an explicit digest), plus the claimed block's exact
        // reference (the replay-termination wake).
        assert_eq!(slot_notifier.num_pending(), 3);
        assert_eq!(ref_notifier.num_pending(), 1);

        // shutdown() aborts and awaits every task: cleanup is deterministic.
        join_set.shutdown().await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 0);
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
        assert_eq!(slot_notifier.num_pending(), 0);
        assert_eq!(ref_notifier.num_pending(), 0);
        // The permit is immediately reusable.
        let permit = h.quotas.try_acquire(h.peer, h.minimal.len(), 3).unwrap();
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

        // A sibling in the claimed block's own slot must not even wake the task:
        // the termination registration is keyed by exact reference, and the claimed
        // block itself is still unaccepted.
        let ref_notifier = h.receiver_dag.read().accepted_ref_notifier();
        let sibling = VerifiedBlock::new_for_test(
            TestBlock::new(11, h.peer.value() as u32)
                .set_timestamp_ms(777_777)
                .build(),
        );
        h.receiver_dag.write().accept_block(sibling);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 1);
        assert_eq!(ref_notifier.num_pending(), 1);

        // The claimed block arrives (as replay would deliver it): the task ends,
        // submits nothing, fetches nothing, and frees its permit.
        h.receiver_dag.write().accept_block(h.block.clone());
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert!(client.fetch_calls.lock().is_empty());
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
    }

    /// Quota isolation across peers: one peer saturating its own lanes must not
    /// impede another peer's admission, and a failed partial acquisition must roll
    /// back the permits it already took.
    #[tokio::test]
    async fn quota_isolation_across_peers_and_rollback() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer_a = context.committee.to_authority_index(1).unwrap();
        let peer_b = context.committee.to_authority_index(2).unwrap();
        let quotas = RecoveryQuotas::with_limits(
            context.clone(),
            RecoveryLimits {
                global_bytes: 1000,
                global_count: 16,
                peer_bytes: 600,
                peer_count: 2,
                waiter_slots: 10,
            },
        );

        // Saturate peer A's count lane.
        let _a1 = quotas.try_acquire(peer_a, 100, 2).unwrap();
        let _a2 = quotas.try_acquire(peer_a, 100, 2).unwrap();
        assert_eq!(
            quotas.try_acquire(peer_a, 100, 2).err(),
            Some(QuotaLimit::PeerCount)
        );
        // Peer B is unaffected by A's saturation.
        let b1 = quotas.try_acquire(peer_b, 100, 2).unwrap();

        // Failed PEER-BYTES admission must roll back the global permit it took:
        // peer B has 500 peer-bytes left but global has 700 left, so a 550-byte
        // acquisition takes global then fails on peer bytes.
        assert_eq!(
            quotas.try_acquire(peer_b, 550, 2).err(),
            Some(QuotaLimit::PeerBytes)
        );
        // If the global permit leaked, only 150 global bytes would remain and this
        // 500-byte admission would fail on GlobalBytes.
        let b2 = quotas.try_acquire(peer_b, 500, 2).unwrap();
        drop(b1);
        drop(b2);
        // Releases return capacity fully: the whole peer-B byte quota is reusable.
        let b3 = quotas.try_acquire(peer_b, 600, 2).unwrap();
        drop(b3);

        // Waiter slots are charged per missing-frontier entry and rolled back with
        // everything else. Peer A's two live permits hold 4 of the 10 slots; taking
        // the remaining 6 exhausts the pool, an over-ask is refused with its earlier
        // permits released, and freed waiters are fully reusable.
        let w1 = quotas.try_acquire(peer_b, 100, 6).unwrap();
        assert_eq!(
            quotas.try_acquire(peer_b, 100, 1).err(),
            Some(QuotaLimit::WaiterSlots)
        );
        drop(w1);
        let w2 = quotas.try_acquire(peer_b, 100, 6).unwrap();
        drop(w2);
    }

    /// Replay can win before the task even starts: with the claimed block already
    /// accepted at spawn, the first register-recheck must conclude AlreadyAccepted and
    /// release the permit — no wait, no fetch, no submission. This pins the
    /// contains_block arm of the recheck for the pre-spawn timing.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn recovery_terminates_when_claimed_block_already_accepted_at_spawn() {
        let h = harness(None);
        let client = Arc::new(RepairOnlyClient::new(vec![]));
        // The harness captured its (now stale) receipt-time reason before this.
        h.receiver_dag.write().accept_block(h.block.clone());
        let mut join_set = tokio::task::JoinSet::new();
        h.spawn(&mut join_set, client.clone());

        let node_metrics = &h.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert!(h.service.lock().handle_send_block.is_empty());
        assert!(client.fetch_calls.lock().is_empty());
        // The permit is immediately reusable.
        let permit = h.quotas.try_acquire(h.peer, h.minimal.len(), 3).unwrap();
        drop(permit);
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
    }
}
