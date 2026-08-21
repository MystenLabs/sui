// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Holding area for slim blocks whose ancestor slots are not all locally resolvable.
//!
//! Such a block cannot be rebuilt, verified, or suspended -- suspension needs a
//! decoded block. It parks here instead: its bytes are held, its missing slots enter
//! an inverted index, and each acceptance decrements its dependents with one map
//! lookup. When an entry's last missing slot fills, the reconstruction worker decodes
//! it and hands the full block to the normal receive path.
//!
//! Everything parked is UNVERIFIED: a claim carries no checked signature until
//! reconstruction succeeds, which is why this state lives in its own container with
//! its own bounds. Admission accepts only claimed rounds inside a GC-anchored window,
//! one entry per slot, under per-peer and total byte quotas; anything refused is
//! dropped, and recovers the same way it does today -- a later block that references
//! it suspends and drives the ordinary missing-ancestor fetch.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use parking_lot::{Mutex, RwLock};
use tokio::sync::broadcast;
use tracing::debug;

use crate::{
    block::{Slot, VerifiedBlock},
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    network::{ExtendedSerializedBlock, SerializedBlockForm, ValidatorNetworkService},
    seen_digests::SeenDigests,
    slim_block::SlimBlockCodec,
};

/// Admission window height above the local accept frontier. The frontier anchor keeps
/// the window live at bootstrap and under commit lag, where the frontier runs far
/// ahead of GC; the cache depth is the natural top, since beyond it the receiver
/// could not resolve the claim's ancestors anyway.
fn window_width(context: &Context) -> Round {
    (context.parameters.dag_state_cached_rounds as Round)
        .max(2 * context.protocol_config.gc_depth())
}

/// Hard cap on resident entries: four GC windows of one-per-slot occupancy.
fn max_pending_entries(context: &Context) -> usize {
    context.committee.size() * 4 * context.protocol_config.gc_depth() as usize
}

/// Per-peer resident-byte quota, bounding what one author can hold here.
const MAX_PARKED_BYTES_PER_PEER: usize = 4 << 20;

/// Node-wide resident-byte cap across all peers.
const MAX_PARKED_BYTES_TOTAL: usize = 128 << 20;

/// One slim block that failed receipt-time decoding, offered for parking.
pub(crate) struct SlimClaim {
    pub(crate) block_ref: BlockRef,
    pub(crate) slim: Bytes,
    pub(crate) excluded_ancestors: Vec<Vec<u8>>,
    pub(crate) peer: AuthorityIndex,
    pub(crate) missing: Vec<Slot>,
}

impl SlimClaim {
    /// Retained bytes including per-item container overhead, so many tiny sidecar
    /// entries cannot ride under the quotas.
    fn charge(&self) -> usize {
        self.slim.len()
            + self
                .excluded_ancestors
                .iter()
                .map(|item| item.len() + std::mem::size_of::<Vec<u8>>())
                .sum::<usize>()
    }
}

/// A slim block waiting for its missing ancestor slots to fill.
struct ParkedSlimBlock {
    slim: Bytes,
    excluded_ancestors: Vec<Vec<u8>>,
    peer: AuthorityIndex,
    /// Remaining missing slots, kept per entry so removal can unlink from the
    /// inverted index without scanning it.
    missing: BTreeSet<Slot>,
    charge: usize,
}

/// Why a claim was not admitted. Refused claims are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmitRefusal {
    /// Claimed round at or below `gc_round`: stale, typically a replay duplicate.
    OutsideWindowLow,
    /// Claimed round above the window top: the receiver is behind the claim.
    OutsideWindowHigh,
    /// A missing slot is at or below `gc_round`: it can never fill, so waiting
    /// cannot recover this block.
    DeadFrontierSlot,
    /// The claimed slot already holds a different entry. The wire only delivers slim
    /// blocks on their author's own stream, so this is the author equivocating
    /// against itself.
    SlotOccupied,
    PeerBytes,
    TotalBytes,
}

impl AdmitRefusal {
    pub(crate) fn metric_label(self) -> &'static str {
        match self {
            AdmitRefusal::OutsideWindowLow => "outside_window_low",
            AdmitRefusal::OutsideWindowHigh => "outside_window_high",
            AdmitRefusal::DeadFrontierSlot => "dead_frontier_slot",
            AdmitRefusal::SlotOccupied => "slot_occupied",
            AdmitRefusal::PeerBytes => "peer_bytes",
            AdmitRefusal::TotalBytes => "total_bytes",
        }
    }
}

/// Resident-byte accounting, shared between the container and popped entries.
/// Atomic so a release never takes a lock, wherever the entry drops.
struct ParkingQuotas {
    context: Arc<Context>,
    per_peer: Vec<std::sync::atomic::AtomicUsize>,
    total: std::sync::atomic::AtomicUsize,
}

impl ParkingQuotas {
    fn charge(&self, peer: AuthorityIndex, charge: usize) {
        use std::sync::atomic::Ordering;
        self.per_peer[peer].fetch_add(charge, Ordering::Relaxed);
        self.total.fetch_add(charge, Ordering::Relaxed);
        // Delta rather than set: concurrent releases would otherwise write totals
        // out of order and strand the gauge on a stale value.
        self.context
            .metrics
            .node_metrics
            .slim_block_parked_bytes
            .add(charge as i64);
    }

    fn release(&self, peer: AuthorityIndex, charge: usize) {
        use std::sync::atomic::Ordering;
        self.per_peer[peer].fetch_sub(charge, Ordering::Relaxed);
        self.total.fetch_sub(charge, Ordering::Relaxed);
        self.context
            .metrics
            .node_metrics
            .slim_block_parked_bytes
            .sub(charge as i64);
    }

    fn peer_bytes(&self, peer: AuthorityIndex) -> usize {
        self.per_peer[peer].load(std::sync::atomic::Ordering::Relaxed)
    }

    fn total_bytes(&self) -> usize {
        self.total.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// An entry whose last missing slot filled, ready to decode. Its byte charge stays
/// counted against the admission caps until the entry drops, worker cancellation
/// included; the release is atomic and takes no lock.
pub(crate) struct ReadyEntry {
    pub(crate) block_ref: BlockRef,
    pub(crate) slim: Bytes,
    pub(crate) excluded_ancestors: Vec<Vec<u8>>,
    pub(crate) peer: AuthorityIndex,
    charge: usize,
    quotas: Arc<ParkingQuotas>,
}

impl ReadyEntry {
    /// The sidecar, leaving the charge in place until the entry drops.
    pub(crate) fn take_excluded_ancestors(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.excluded_ancestors)
    }
}

impl Drop for ReadyEntry {
    fn drop(&mut self) {
        self.quotas.release(self.peer, self.charge);
    }
}

pub(crate) struct PendingReconstructions {
    context: Arc<Context>,
    pending: BTreeMap<BlockRef, ParkedSlimBlock>,
    /// Inverted index: missing slot -> the parked entries waiting on it.
    missing_slots: BTreeMap<Slot, BTreeSet<BlockRef>>,
    /// One entry per claimed slot, whichever digest claimed it first.
    occupancy: BTreeMap<Slot, BlockRef>,
    quotas: Arc<ParkingQuotas>,
}

impl PendingReconstructions {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let peers = context.committee.size();
        let quotas = Arc::new(ParkingQuotas {
            context: context.clone(),
            per_peer: (0..peers).map(|_| Default::default()).collect(),
            total: Default::default(),
        });
        Self {
            context,
            pending: BTreeMap::new(),
            missing_slots: BTreeMap::new(),
            occupancy: BTreeMap::new(),
            quotas,
        }
    }

    /// Admits a claim or says why not.
    pub(crate) fn try_admit(
        &mut self,
        mut claim: SlimClaim,
        gc_round: Round,
        local_round: Round,
    ) -> Result<(), AdmitRefusal> {
        // Same sidecar bound the receive path enforces, into a fresh allocation --
        // an in-place truncation or collect can retain the oversized buffer -- so
        // the charge checked below is exactly what would become resident.
        let limit = 2 * self.context.committee.size();
        if claim.excluded_ancestors.len() > limit {
            let mut bounded = Vec::with_capacity(limit);
            bounded.extend(claim.excluded_ancestors.drain(..limit));
            claim.excluded_ancestors = bounded;
        }
        if let Some(refusal) = self.admission_refusal(&claim, gc_round, local_round) {
            return Err(refusal);
        }
        if self.pending.contains_key(&claim.block_ref) {
            // Same ref re-delivered (resubscription race): already parked, and the
            // charge must not be taken twice.
            return Ok(());
        }

        let SlimClaim {
            block_ref,
            slim,
            excluded_ancestors,
            peer,
            missing,
        } = claim;
        // Detach from the wire buffer: the incoming `Bytes` shares the receive
        // allocation, and a parked clone would pin all of it.
        let slim = Bytes::copy_from_slice(&slim);
        let charge = slim.len()
            + excluded_ancestors
                .iter()
                .map(|item| item.len() + std::mem::size_of::<Vec<u8>>())
                .sum::<usize>();
        self.quotas.charge(peer, charge);
        let missing: BTreeSet<Slot> = missing.into_iter().collect();
        for slot in &missing {
            self.missing_slots
                .entry(*slot)
                .or_default()
                .insert(block_ref);
        }
        self.occupancy.insert(Slot::from(block_ref), block_ref);
        self.pending.insert(
            block_ref,
            ParkedSlimBlock {
                slim,
                excluded_ancestors,
                peer,
                missing,
                charge,
            },
        );
        self.update_gauges();
        Ok(())
    }

    fn admission_refusal(
        &self,
        claim: &SlimClaim,
        gc_round: Round,
        local_round: Round,
    ) -> Option<AdmitRefusal> {
        let window_top = local_round.saturating_add(window_width(&self.context));
        if claim.block_ref.round <= gc_round {
            return Some(AdmitRefusal::OutsideWindowLow);
        }
        if claim.block_ref.round > window_top {
            return Some(AdmitRefusal::OutsideWindowHigh);
        }
        if claim.missing.iter().any(|slot| slot.round <= gc_round) {
            return Some(AdmitRefusal::DeadFrontierSlot);
        }
        if self.pending.len() >= max_pending_entries(&self.context) {
            return Some(AdmitRefusal::TotalBytes);
        }
        if self.occupancy.contains_key(&Slot::from(claim.block_ref)) {
            // Idempotent for the identical ref: already parked is success.
            if self.pending.contains_key(&claim.block_ref) {
                return None;
            }
            return Some(AdmitRefusal::SlotOccupied);
        }
        let charge = claim.charge();
        if self.quotas.peer_bytes(claim.peer) + charge > MAX_PARKED_BYTES_PER_PEER {
            return Some(AdmitRefusal::PeerBytes);
        }
        if self.quotas.total_bytes() + charge > MAX_PARKED_BYTES_TOTAL {
            return Some(AdmitRefusal::TotalBytes);
        }
        None
    }

    /// Snapshot of currently-missing slots, for reconciling against DagState after
    /// the accepted-block broadcast reports lag.
    pub(crate) fn missing_slot_snapshot(&self) -> Vec<Slot> {
        self.missing_slots.keys().copied().collect()
    }

    /// Marks the slots of `accepted` filled and pops the entries with nothing left
    /// to wait for.
    pub(crate) fn on_blocks_accepted(&mut self, accepted: &[BlockRef]) -> Vec<ReadyEntry> {
        let mut ready = Vec::new();
        for block_ref in accepted {
            let slot = Slot::from(*block_ref);
            let Some(dependents) = self.missing_slots.remove(&slot) else {
                continue;
            };
            for dependent in dependents {
                let entry = self
                    .pending
                    .get_mut(&dependent)
                    .expect("index entries always have a parked block");
                entry.missing.remove(&slot);
                if entry.missing.is_empty() {
                    let entry = self
                        .remove_entry_keeping_charge(&dependent)
                        .expect("checked above");
                    ready.push(ReadyEntry {
                        block_ref: dependent,
                        slim: entry.slim,
                        excluded_ancestors: entry.excluded_ancestors,
                        peer: entry.peer,
                        charge: entry.charge,
                        quotas: self.quotas.clone(),
                    });
                }
            }
        }
        if !ready.is_empty() {
            self.update_gauges();
        }
        ready
    }

    /// Drops entries GC has made unrecoverable: the claim itself now below the
    /// window, or a missing slot that can no longer fill. Returns how many died.
    pub(crate) fn on_gc(&mut self, gc_round: Round) -> usize {
        let dead: Vec<BlockRef> = self
            .pending
            .iter()
            .filter(|(block_ref, entry)| {
                block_ref.round <= gc_round
                    || entry.missing.iter().any(|slot| slot.round <= gc_round)
            })
            .map(|(block_ref, _)| *block_ref)
            .collect();
        for block_ref in &dead {
            let entry = self.remove_entry_keeping_charge(block_ref).expect("listed");
            self.quotas.release(entry.peer, entry.charge);
        }
        if !dead.is_empty() {
            self.update_gauges();
        }
        dead.len()
    }

    /// Removes an entry from the maps while keeping its bytes charged, for handoff
    /// to the worker.
    fn remove_entry_keeping_charge(&mut self, block_ref: &BlockRef) -> Option<ParkedSlimBlock> {
        let entry = self.pending.remove(block_ref)?;
        for slot in &entry.missing {
            if let Some(dependents) = self.missing_slots.get_mut(slot) {
                dependents.remove(block_ref);
                if dependents.is_empty() {
                    self.missing_slots.remove(slot);
                }
            }
        }
        self.occupancy.remove(&Slot::from(*block_ref));
        self.update_gauges();
        Some(entry)
    }

    fn update_gauges(&self) {
        self.context
            .metrics
            .node_metrics
            .slim_block_parked_entries
            .set(self.pending.len() as i64);
    }
}

/// Consumes Core's accepted-block broadcast, plus entries the subscriber found
/// already complete at admission, and finishes parked entries whose slots have
/// filled. Reconstructed blocks re-enter through `handle_send_block` like any
/// received block; entries that still fail to decode are dropped.
///
/// GC cleanup piggybacks on the broadcast: a commit that advances GC with no later
/// acceptance leaves dead entries resident until the next accepted block, which is
/// also the next moment anything could change here.
pub(crate) async fn run_reconstruction_worker<S: ValidatorNetworkService>(
    context: Arc<Context>,
    codec: Arc<SlimBlockCodec>,
    dag_state: Weak<RwLock<DagState>>,
    authority_service: Weak<S>,
    pending: Arc<Mutex<PendingReconstructions>>,
    seen_digests: Arc<SeenDigests>,
    mut accepted: broadcast::Receiver<VerifiedBlock>,
    mut reconciled: tokio::sync::mpsc::UnboundedReceiver<Vec<ReadyEntry>>,
) {
    let mut last_gc_round: Round = 0;
    loop {
        let mut refs = Vec::new();
        let mut lagged = false;
        tokio::select! {
            received = accepted.recv() => match received {
                Ok(block) => refs.push(block.reference()),
                // Falling behind the broadcast must not lose wake-ups: reconcile
                // every pending slot against DagState instead.
                Err(broadcast::error::RecvError::Lagged(_)) => lagged = true,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            entries = reconciled.recv() => {
                let Some(entries) = entries else { return };
                for entry in entries {
                    finish_entry(&context, &codec, &dag_state, &authority_service, &seen_digests, entry)
                        .await;
                }
                continue;
            }
        }
        loop {
            match accepted.try_recv() {
                Ok(block) => refs.push(block.reference()),
                Err(broadcast::error::TryRecvError::Lagged(_)) => lagged = true,
                Err(_) => break,
            }
        }

        let Some(dag_state_strong) = dag_state.upgrade() else {
            return;
        };
        let gc_round = {
            let guard = dag_state_strong.read();
            if lagged {
                let missing = pending.lock().missing_slot_snapshot();
                refs.extend(missing.iter().flat_map(|slot| {
                    guard
                        .get_uncommitted_blocks_at_slot(*slot)
                        .into_iter()
                        .map(|b| b.reference())
                        .take(1)
                }));
            }
            guard.gc_round()
        };
        drop(dag_state_strong);

        let ready = {
            let mut pending = pending.lock();
            let ready = pending.on_blocks_accepted(&refs);
            if gc_round > last_gc_round {
                last_gc_round = gc_round;
                let dropped = pending.on_gc(gc_round);
                if dropped > 0 {
                    context
                        .metrics
                        .node_metrics
                        .slim_block_park_outcomes
                        .with_label_values(&["gc_dropped"])
                        .inc_by(dropped as u64);
                }
            }
            ready
        };

        for entry in ready {
            finish_entry(
                &context,
                &codec,
                &dag_state,
                &authority_service,
                &seen_digests,
                entry,
            )
            .await;
        }
    }
}

/// Reconstructs and delivers one entry, recording its outcome. The entry's charge
/// releases when it drops here.
async fn finish_entry<S: ValidatorNetworkService>(
    context: &Context,
    codec: &SlimBlockCodec,
    dag_state: &Weak<RwLock<DagState>>,
    authority_service: &Weak<S>,
    seen_digests: &SeenDigests,
    entry: ReadyEntry,
) {
    let outcome =
        reconstruct_and_deliver(codec, dag_state, authority_service, seen_digests, entry).await;
    context
        .metrics
        .node_metrics
        .slim_block_park_outcomes
        .with_label_values(&[outcome])
        .inc();
}

/// Decodes one ready entry against current state and hands it to the receive path.
/// Returns the outcome label.
async fn reconstruct_and_deliver<S: ValidatorNetworkService>(
    codec: &SlimBlockCodec,
    dag_state: &Weak<RwLock<DagState>>,
    authority_service: &Weak<S>,
    seen_digests: &SeenDigests,
    mut entry: ReadyEntry,
) -> &'static str {
    let Some(dag_state) = dag_state.upgrade() else {
        return "shutdown";
    };
    let serialized = match codec.decode(&entry.slim, entry.peer, &dag_state, seen_digests) {
        Ok((_signed, serialized)) => serialized,
        // A slot resolvable at admission can be collected before the wake; the
        // block then recovers the ordinary way, through a dependent's suspension.
        Err(error) => {
            debug!(
                "Dropping parked block {} from {}: {}",
                entry.block_ref, entry.peer, error
            );
            return error.metric_label();
        }
    };
    drop(dag_state);
    let Some(authority_service) = authority_service.upgrade() else {
        return "shutdown";
    };
    let block = ExtendedSerializedBlock {
        block: SerializedBlockForm::Full(serialized),
        excluded_ancestors: entry.take_excluded_ancestors(),
    };
    match authority_service.handle_send_block(entry.peer, block).await {
        Ok(()) => "reconstructed",
        Err(ConsensusError::BlockRejected { .. }) => "rejected",
        Err(_) => "receive_error",
    }
}

#[cfg(test)]
mod tests {
    use consensus_types::block::BlockDigest;
    use rand::{RngCore as _, SeedableRng as _, rngs::StdRng};

    use super::*;
    use crate::context::Context;

    fn fixture() -> (Arc<Context>, Arc<Mutex<PendingReconstructions>>) {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let pending = Arc::new(Mutex::new(PendingReconstructions::new(context.clone())));
        (context, pending)
    }

    fn r(rng: &mut StdRng, round: Round, author: u32) -> BlockRef {
        let mut digest = [0u8; 32];
        rng.fill_bytes(&mut digest);
        BlockRef::new(
            round,
            AuthorityIndex::new_for_test(author),
            BlockDigest(digest),
        )
    }

    fn claim(block_ref: BlockRef, missing: Vec<Slot>, payload: usize) -> SlimClaim {
        SlimClaim {
            block_ref,
            slim: Bytes::from(vec![0u8; payload]),
            excluded_ancestors: vec![],
            peer: block_ref.author,
            missing,
        }
    }

    /// One acceptance decrements every dependent of that slot; an entry pops only
    /// when its last missing slot fills, and pops exactly once.
    #[test]
    fn acceptance_cascades_through_the_slot_index() {
        let mut rng = StdRng::seed_from_u64(7);
        let (_context, pending) = fixture();
        let shared = Slot::new(5, AuthorityIndex::new_for_test(2));
        let only = r(&mut rng, 6, 1);
        let both = r(&mut rng, 6, 3);
        let extra = Slot::new(5, AuthorityIndex::new_for_test(0));
        pending
            .lock()
            .try_admit(claim(only, vec![shared], 10), 0, 6)
            .unwrap();
        pending
            .lock()
            .try_admit(claim(both, vec![shared, extra], 10), 0, 6)
            .unwrap();

        let filler = r(&mut rng, 5, 2);
        let ready = pending.lock().on_blocks_accepted(&[filler]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].block_ref, only);

        let ready = pending.lock().on_blocks_accepted(&[r(&mut rng, 5, 0)]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].block_ref, both);

        // Nothing left: a re-accept of the same slot pops nothing.
        assert!(pending.lock().on_blocks_accepted(&[filler]).is_empty());
    }

    /// Each refusal fires on its condition; an admitted duplicate is idempotent and
    /// a different digest at an occupied slot is refused.
    #[test]
    fn admission_refusals() {
        let mut rng = StdRng::seed_from_u64(8);
        let (context, pending) = fixture();
        let gc_round = 10;
        let local_round = 20;
        let missing = vec![Slot::new(12, AuthorityIndex::new_for_test(2))];

        let stale = r(&mut rng, gc_round, 1);
        assert_eq!(
            pending
                .lock()
                .try_admit(claim(stale, missing.clone(), 10), gc_round, local_round),
            Err(AdmitRefusal::OutsideWindowLow)
        );

        let window_top = local_round + window_width(&context);
        let ahead = r(&mut rng, window_top + 1, 1);
        assert_eq!(
            pending
                .lock()
                .try_admit(claim(ahead, missing.clone(), 10), gc_round, local_round),
            Err(AdmitRefusal::OutsideWindowHigh)
        );

        let dead_slot = vec![Slot::new(gc_round, AuthorityIndex::new_for_test(2))];
        let dead = r(&mut rng, 15, 1);
        assert_eq!(
            pending
                .lock()
                .try_admit(claim(dead, dead_slot, 10), gc_round, local_round),
            Err(AdmitRefusal::DeadFrontierSlot)
        );

        let parked = r(&mut rng, 15, 1);
        pending
            .lock()
            .try_admit(claim(parked, missing.clone(), 10), gc_round, local_round)
            .unwrap();
        // Identical ref again: success, not a second charge.
        pending
            .lock()
            .try_admit(claim(parked, missing.clone(), 10), gc_round, local_round)
            .unwrap();
        assert_eq!(pending.lock().quotas.total_bytes(), 10);
        // Different digest at the same slot: the author equivocating against itself.
        let equivocation = BlockRef::new(parked.round, parked.author, BlockDigest([9u8; 32]));
        assert_eq!(
            pending.lock().try_admit(
                claim(equivocation, missing.clone(), 10),
                gc_round,
                local_round
            ),
            Err(AdmitRefusal::SlotOccupied)
        );

        let big = r(&mut rng, 16, 1);
        assert_eq!(
            pending.lock().try_admit(
                claim(big, missing.clone(), MAX_PARKED_BYTES_PER_PEER),
                gc_round,
                local_round
            ),
            Err(AdmitRefusal::PeerBytes)
        );
    }

    /// GC kills entries whose claim fell below it or whose missing slot can no
    /// longer fill, and returns their bytes to the quotas.
    #[test]
    fn gc_drops_unrecoverable_entries_and_releases_bytes() {
        let mut rng = StdRng::seed_from_u64(9);
        let (_context, pending) = fixture();
        let doomed = r(&mut rng, 20, 1);
        let survivor = r(&mut rng, 30, 2);
        pending
            .lock()
            .try_admit(
                claim(
                    doomed,
                    vec![Slot::new(11, AuthorityIndex::new_for_test(3))],
                    10,
                ),
                5,
                20,
            )
            .unwrap();
        pending
            .lock()
            .try_admit(
                claim(
                    survivor,
                    vec![Slot::new(25, AuthorityIndex::new_for_test(3))],
                    10,
                ),
                5,
                30,
            )
            .unwrap();
        assert_eq!(pending.lock().quotas.total_bytes(), 20);

        // `doomed` has a live claim round but a missing slot at 11 <= 12: only the
        // dead-slot condition can kill it.
        assert_eq!(pending.lock().on_gc(12), 1);
        let guard = pending.lock();
        assert_eq!(guard.quotas.total_bytes(), 10);
        assert_eq!(guard.pending.len(), 1);
        // The dead entry left no residue in the index or occupancy.
        assert!(guard.pending.contains_key(&survivor));
        assert!(!guard.occupancy.contains_key(&Slot::from(doomed)));
        assert!(
            !guard
                .missing_slots
                .contains_key(&Slot::new(11, AuthorityIndex::new_for_test(3)))
        );
    }

    /// An oversize sidecar is bounded into a fresh allocation, and the stored
    /// charge is what the bounded form retains.
    #[test]
    fn oversize_sidecars_are_bounded_before_charging() {
        let mut rng = StdRng::seed_from_u64(11);
        let (context, pending) = fixture();
        let limit = 2 * context.committee.size();
        let parked = r(&mut rng, 6, 1);
        let mut oversized = claim(
            parked,
            vec![Slot::new(5, AuthorityIndex::new_for_test(2))],
            0,
        );
        oversized.excluded_ancestors = vec![vec![7u8; 4]; limit * 100];
        pending.lock().try_admit(oversized, 0, 6).unwrap();

        let expected = limit * (4 + std::mem::size_of::<Vec<u8>>());
        assert_eq!(pending.lock().quotas.total_bytes(), expected);

        let ready = pending.lock().on_blocks_accepted(&[r(&mut rng, 5, 2)]);
        assert_eq!(ready[0].excluded_ancestors.len(), limit);
        assert!(ready[0].excluded_ancestors.capacity() <= limit);
        drop(ready);
        assert_eq!(pending.lock().quotas.total_bytes(), 0);
    }

    /// A popped entry keeps its bytes charged against admission until it drops,
    /// wherever that drop happens.
    #[test]
    fn ready_entries_hold_their_charge_until_dropped() {
        let mut rng = StdRng::seed_from_u64(10);
        let (_context, pending) = fixture();
        let parked = r(&mut rng, 6, 1);
        let missing = Slot::new(5, AuthorityIndex::new_for_test(2));
        pending
            .lock()
            .try_admit(claim(parked, vec![missing], 10), 0, 6)
            .unwrap();

        let ready = pending.lock().on_blocks_accepted(&[r(&mut rng, 5, 2)]);
        assert_eq!(ready.len(), 1);
        assert_eq!(
            pending.lock().quotas.total_bytes(),
            10,
            "still charged while in flight"
        );

        drop(ready);
        assert_eq!(pending.lock().quotas.total_bytes(), 0);
    }
}
