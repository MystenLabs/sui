// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Slot-keyed pending_reconstructions for minimal blocks that cannot be inflated at receipt —
//! block-manager suspension, transposed to slots, living beside it.
//!
//! A minimal block whose ancestor slots are not all locally accepted cannot be
//! reconstructed, verified, or suspended. It parks here instead: its bytes are held,
//! its missing slots enter an inverted index, and every block acceptance decrements
//! its dependents with one map lookup — the same shape `BlockManager` uses for
//! suspended full blocks, rather than per-block tasks and notifier registrations.
//! When an entry's last missing slot fills, it is handed to a bounded reconstruction
//! worker; the reconstructed full block then enters the normal verification and
//! acceptance pipeline like any other received block.
//!
//! Bounds are structural, not tuned. Admission accepts only claimed rounds in
//! `(gc_round, gc_round + 2·gc_depth]` — anchored to GC so the window has provable
//! width, and so pending_reconstructions stops growing when commits stall instead of absorbing the
//! stall. One entry per claimed slot (the wire protocol only delivers minimal blocks
//! on their author's own stream, so a second claim for a slot is only ever the
//! author equivocating against itself). A per-peer byte quota bounds any one
//! author's residency and an aggregate cap bounds the node; both refuse at admission
//! rather than evict, which removes eviction-thrash as an attack.
//!
//! Everything here is UNVERIFIED: claims carry no checked signature until
//! reconstruction succeeds. That is why this state lives in its own container with
//! its own bounds, and why refusals must never be labelled equivocation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::UnboundedSender;

use crate::{block::Slot, context::Context};

/// Admission window: above `gc_round` (below it the claim is already obsolete) and
/// at most one GC depth above the LOCAL accept frontier. Anchoring the top to the
/// frontier rather than to `gc_round` matters at bootstrap and under commit lag,
/// where the frontier runs arbitrarily far ahead of GC and a gc-anchored top would
/// refuse every live block. The frontier anchor alone does not bound the entry
/// count when commits stall (the frontier keeps moving while GC does not), so an
/// explicit entry cap supplies the bound the window no longer proves.
fn window_width(context: &Context) -> Round {
    context.protocol_config.gc_depth()
}

/// Hard cap on resident entries: four GC windows of one-per-slot occupancy. Honest
/// steady state sits two orders of magnitude below this; reaching it means commits
/// have stalled for minutes, and refusing then is the correct backpressure.
fn max_pending_entries(context: &Context) -> usize {
    context.committee.size() * 4 * context.protocol_config.gc_depth() as usize
}

/// Per-peer resident-byte quota. Honest steady state per peer is a few hundred KB
/// (park rate × residency × payload); this is ~16× that, and it bounds the damage a
/// single author can do by stuffing its own window with maximum-size claims.
const MAX_PARKED_BYTES_PER_PEER: usize = 4 << 20;

/// Node-wide resident-byte cap across all peers — the backstop that keeps worst-case
/// aggregate memory two orders of magnitude below what per-peer quotas alone permit.
const MAX_PARKED_BYTES_TOTAL: usize = 128 << 20;

/// Held sidecars for blocks that were NOT parked (refused admission or terminal
/// without acceptance). Propagation hints must survive the block's own refusal; they
/// are delivered when the block is later accepted through any path. Bounded per
/// entry by the receive path's sidecar size cap, and in count here.
const MAX_HELD_SIDECARS: usize = 8192;

/// Byte bound on held sidecars, enforced with the entry bound: entries alone do not
/// bound memory when each sidecar can be transport-message sized.
const MAX_HELD_SIDECAR_BYTES: usize = 8 << 20;

pub(crate) struct PendingMinimal {
    pub(crate) minimal: Bytes,
    pub(crate) excluded_ancestors: Vec<Vec<u8>>,
    pub(crate) peer: AuthorityIndex,
    pub(crate) parked_at_round: Round,
    /// Full remaining missing-slot set — kept per entry so removal can unlink from
    /// the inverted index without scanning it.
    missing: BTreeSet<Slot>,
    charge: usize,
}

impl PendingMinimal {
    fn charge(minimal: &Bytes, excluded_ancestors: &[Vec<u8>]) -> usize {
        minimal.len() + excluded_ancestors.iter().map(Vec::len).sum::<usize>()
    }
}

/// Why a claim was not admitted. `metric_label` values deliberately avoid the word
/// "equivocation": claims are unverified here, and a slot collision is not evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmitRefusal {
    /// Claimed round outside `(gc_round, gc_round + window]`.
    OutsideWindow,
    /// A frontier slot is at or below `gc_round`: it can never fill, so slot-waiting
    /// cannot recover this block. The caller routes it to the exact-fetch lane.
    DeadFrontierSlot,
    /// The claimed slot already holds an entry (only its own author can cause this).
    SlotOccupied,
    PeerBytes,
    TotalBytes,
}

impl AdmitRefusal {
    pub(crate) fn metric_label(self) -> &'static str {
        match self {
            AdmitRefusal::OutsideWindow => "outside_window",
            AdmitRefusal::DeadFrontierSlot => "dead_frontier_slot",
            AdmitRefusal::SlotOccupied => "slot_occupied",
            AdmitRefusal::PeerBytes => "peer_bytes",
            AdmitRefusal::TotalBytes => "total_bytes",
        }
    }
}

/// An entry whose frontier has filled, ready for reconstruction off-thread.
pub(crate) struct ReadyEntry {
    pub(crate) claimed_ref: BlockRef,
    pub(crate) minimal: Bytes,
    pub(crate) excluded_ancestors: Vec<Vec<u8>>,
    pub(crate) peer: AuthorityIndex,
    pub(crate) parked_at_round: Round,
    /// Byte charge carried until the worker reaches a terminal state, so queued and
    /// in-flight reconstructions stay inside the admission caps and a lagging worker
    /// backpressures admission instead of letting the effects queue grow unbounded.
    pub(crate) charge: usize,
}

/// Shared handle DagState uses to drive pending_reconstructions from its acceptance and GC
/// choke points. The effects channel is unbounded and drained by the
/// reconstruction worker; sends never block under the DagState write guard.
#[derive(Clone)]
pub(crate) struct ReconstructionHook {
    pub(crate) pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    pub(crate) effects: UnboundedSender<AcceptanceEffects>,
}

/// Outcome of a batch of acceptances or a GC sweep, applied by the worker outside
/// any DagState or pending_reconstructions lock.
#[derive(Default)]
pub(crate) struct AcceptanceEffects {
    /// Entries whose last missing slot filled: dispatch to the reconstruction worker.
    pub(crate) ready: Vec<ReadyEntry>,
    /// Sidecars whose anchor block just got accepted through another path: deliver
    /// through `handle_excluded_ancestors` (the anchor precondition now holds).
    pub(crate) deliverable_sidecars: Vec<(AuthorityIndex, BlockRef, Vec<Vec<u8>>)>,
    /// Entries whose frontier died at GC: only the exact-fetch lane can finish them.
    pub(crate) frontier_dead: Vec<BlockRef>,
}

impl AcceptanceEffects {
    pub(crate) fn is_empty(&self) -> bool {
        self.ready.is_empty()
            && self.deliverable_sidecars.is_empty()
            && self.frontier_dead.is_empty()
    }
}

/// Per-authority lowest pending missing-slot round, shared with the synchronizer so
/// the periodic scheduler can lower its `fetch_after_rounds` vector. `Round::MAX`
/// means no pending slot for that authority. Updated under the core thread on every
/// mutation; read lock-free-ish by the scheduler.
pub(crate) type PendingSlotFloor = Arc<RwLock<Vec<Round>>>;

pub(crate) struct PendingReconstructions {
    context: Arc<Context>,
    pending: BTreeMap<BlockRef, PendingMinimal>,
    /// Inverted index: missing slot → entries waiting on it.
    missing_slots: BTreeMap<Slot, BTreeSet<BlockRef>>,
    /// One entry per claimed slot.
    occupancy: BTreeMap<Slot, BlockRef>,
    /// Sidecars held for refused / terminal-without-acceptance blocks, FIFO-bounded.
    held_sidecars: BTreeMap<BlockRef, (AuthorityIndex, Vec<Vec<u8>>)>,
    held_order: Vec<BlockRef>,
    held_bytes: usize,
    per_peer_bytes: Vec<usize>,
    total_bytes: usize,
    /// Entries dispatched to the worker and not yet terminal; counted against the
    /// entry cap alongside `pending`.
    in_flight: usize,
    slot_floor: PendingSlotFloor,
}

impl PendingReconstructions {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let committee_size = context.committee.size();
        Self {
            context,
            pending: BTreeMap::new(),
            missing_slots: BTreeMap::new(),
            occupancy: BTreeMap::new(),
            held_sidecars: BTreeMap::new(),
            held_order: Vec::new(),
            held_bytes: 0,
            per_peer_bytes: vec![0; committee_size],
            total_bytes: 0,
            in_flight: 0,
            slot_floor: Arc::new(RwLock::new(vec![Round::MAX; committee_size])),
        }
    }

    /// Closes the read-then-admit race: slots read as missing before admission may
    /// have been accepted in between, and duplicate acceptance never re-fires the
    /// hook. Called by the admitter right after a successful admit with the refs of
    /// any of the entry's missing slots that are now filled.
    pub(crate) fn recheck_filled_slots(&mut self, filled: &[BlockRef]) -> AcceptanceEffects {
        self.on_blocks_accepted(filled)
    }

    pub(crate) fn slot_floor(&self) -> PendingSlotFloor {
        self.slot_floor.clone()
    }

    /// Admits a claim or says why not. On refusal the sidecar is still held so the
    /// propagation hints survive; `DeadFrontierSlot` refusals should additionally be
    /// routed to the exact-fetch lane by the caller.
    pub(crate) fn try_admit(
        &mut self,
        claimed_ref: BlockRef,
        minimal: Bytes,
        excluded_ancestors: Vec<Vec<u8>>,
        peer: AuthorityIndex,
        missing: Vec<Slot>,
        gc_round: Round,
        local_round: Round,
    ) -> Result<(), AdmitRefusal> {
        let refusal = self.admission_refusal(
            &claimed_ref,
            &minimal,
            &excluded_ancestors,
            peer,
            &missing,
            gc_round,
            local_round,
        );
        if let Some(refusal) = refusal {
            self.hold_sidecar(claimed_ref, peer, excluded_ancestors);
            return Err(refusal);
        }
        if self.pending.contains_key(&claimed_ref) {
            // Same ref re-delivered (resubscription race): already parked, and the
            // charge must not be taken twice.
            return Ok(());
        }

        let charge = PendingMinimal::charge(&minimal, &excluded_ancestors);
        self.per_peer_bytes[peer] += charge;
        self.total_bytes += charge;
        let missing: BTreeSet<Slot> = missing.into_iter().collect();
        for slot in &missing {
            self.missing_slots
                .entry(*slot)
                .or_default()
                .insert(claimed_ref);
        }
        self.occupancy.insert(Slot::from(claimed_ref), claimed_ref);
        self.pending.insert(
            claimed_ref,
            PendingMinimal {
                minimal,
                excluded_ancestors,
                peer,
                parked_at_round: local_round,
                missing,
                charge,
            },
        );
        self.rebuild_slot_floor();
        self.update_gauges();
        Ok(())
    }

    fn admission_refusal(
        &self,
        claimed_ref: &BlockRef,
        minimal: &Bytes,
        excluded_ancestors: &[Vec<u8>],
        peer: AuthorityIndex,
        missing: &[Slot],
        gc_round: Round,
        local_round: Round,
    ) -> Option<AdmitRefusal> {
        let window_top = local_round.saturating_add(window_width(&self.context));
        if claimed_ref.round <= gc_round || claimed_ref.round > window_top {
            return Some(AdmitRefusal::OutsideWindow);
        }
        if self.pending.len() + self.in_flight >= max_pending_entries(&self.context) {
            return Some(AdmitRefusal::TotalBytes);
        }
        if missing.iter().any(|slot| slot.round <= gc_round) {
            return Some(AdmitRefusal::DeadFrontierSlot);
        }
        if self.occupancy.contains_key(&Slot::from(*claimed_ref)) {
            // Idempotent for the identical ref (re-delivery after a resubscription
            // race): already parked is success, not refusal.
            if self.pending.contains_key(claimed_ref) {
                return None;
            }
            return Some(AdmitRefusal::SlotOccupied);
        }
        let charge = PendingMinimal::charge(minimal, excluded_ancestors);
        if self.per_peer_bytes[peer] + charge > MAX_PARKED_BYTES_PER_PEER {
            return Some(AdmitRefusal::PeerBytes);
        }
        if self.total_bytes + charge > MAX_PARKED_BYTES_TOTAL {
            return Some(AdmitRefusal::TotalBytes);
        }
        None
    }

    /// One lookup per accepted block: unblocks dependents, and if the accepted block
    /// IS a parked or sidecar-held ref, resolves that entry. Call AFTER the blocks
    /// are visible in DagState, so dispatched reconstructions can see them.
    pub(crate) fn on_blocks_accepted(&mut self, accepted: &[BlockRef]) -> AcceptanceEffects {
        let mut effects = AcceptanceEffects::default();
        for block_ref in accepted {
            // The accepted block may itself be a parked claim (arrived via fetch,
            // replay, or commit sync first): the parked copy is superseded, and its
            // sidecar becomes deliverable now that the anchor is accepted.
            if let Some(entry) = self.remove_entry(block_ref) {
                self.observe_residency(&entry, "superseded", block_ref.round);
                effects.deliverable_sidecars.push((
                    entry.peer,
                    *block_ref,
                    entry.excluded_ancestors,
                ));
            }
            if let Some((peer, sidecar)) = self.held_sidecars.remove(block_ref) {
                effects
                    .deliverable_sidecars
                    .push((peer, *block_ref, sidecar));
            }

            let slot = Slot::from(*block_ref);
            let Some(dependents) = self.missing_slots.remove(&slot) else {
                continue;
            };
            for dependent in dependents {
                let Some(entry) = self.pending.get_mut(&dependent) else {
                    continue;
                };
                entry.missing.remove(&slot);
                if entry.missing.is_empty() {
                    let entry = self
                        .remove_entry_keeping_charge(&dependent)
                        .expect("entry exists");
                    self.in_flight += 1;
                    self.observe_residency(&entry, "reconstructing", block_ref.round);
                    effects.ready.push(ReadyEntry {
                        claimed_ref: dependent,
                        minimal: entry.minimal,
                        excluded_ancestors: entry.excluded_ancestors,
                        peer: entry.peer,
                        parked_at_round: entry.parked_at_round,
                        charge: entry.charge,
                    });
                }
            }
        }
        if !effects.ready.is_empty() || !effects.deliverable_sidecars.is_empty() {
            self.rebuild_slot_floor();
            self.update_gauges();
        }
        effects
    }

    /// GC sweep: entries whose claimed round crossed GC are obsolete; live entries
    /// with a frontier slot that crossed GC can never fill by waiting and are
    /// returned for exact-lane routing.
    pub(crate) fn on_gc(&mut self, gc_round: Round, local_round: Round) -> Vec<BlockRef> {
        let obsolete: Vec<BlockRef> = self
            .pending
            .iter()
            .filter(|(r, _)| r.round <= gc_round)
            .map(|(r, _)| *r)
            .collect();
        for block_ref in &obsolete {
            let entry = self.remove_entry(block_ref).expect("entry exists");
            // Obsolete means below GC: nothing will accept it, so the sidecar is
            // dropped with it — same as upstream drops sub-GC blocks wholesale.
            self.observe_residency(&entry, "obsolete", local_round);
        }
        let frontier_dead: Vec<BlockRef> = self
            .pending
            .iter()
            .filter(|(_, e)| e.missing.iter().any(|slot| slot.round <= gc_round))
            .map(|(r, _)| *r)
            .collect();
        for block_ref in &frontier_dead {
            let entry = self.remove_entry(block_ref).expect("entry exists");
            self.hold_sidecar(*block_ref, entry.peer, entry.excluded_ancestors.clone());
            self.observe_residency(&entry, "frontier_dead", local_round);
        }
        self.held_sidecars.retain(|r, (_, sidecar)| {
            let keep = r.round > gc_round;
            if !keep {
                self.held_bytes -= sidecar.iter().map(Vec::len).sum::<usize>();
            }
            keep
        });
        self.held_order.retain(|r| r.round > gc_round);
        if !obsolete.is_empty() || !frontier_dead.is_empty() {
            self.rebuild_slot_floor();
            self.update_gauges();
        }
        frontier_dead
    }

    /// Removes an entry and releases its byte charge immediately (terminal here).
    fn remove_entry(&mut self, block_ref: &BlockRef) -> Option<PendingMinimal> {
        let entry = self.remove_entry_keeping_charge(block_ref)?;
        self.per_peer_bytes[entry.peer] -= entry.charge;
        self.total_bytes -= entry.charge;
        Some(entry)
    }

    /// Removes an entry from every index but keeps its byte charge: used when the
    /// bytes live on in a queued/in-flight reconstruction. `release` pairs with it.
    fn remove_entry_keeping_charge(&mut self, block_ref: &BlockRef) -> Option<PendingMinimal> {
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
        Some(entry)
    }

    /// Terminal release for a dispatched reconstruction, whatever its outcome.
    pub(crate) fn release(&mut self, peer: AuthorityIndex, charge: usize) {
        self.per_peer_bytes[peer] -= charge;
        self.total_bytes -= charge;
        self.in_flight = self.in_flight.saturating_sub(1);
        self.update_gauges();
    }

    pub(crate) fn hold_sidecar(
        &mut self,
        block_ref: BlockRef,
        peer: AuthorityIndex,
        sidecar: Vec<Vec<u8>>,
    ) {
        if sidecar.is_empty() || self.held_sidecars.contains_key(&block_ref) {
            return;
        }
        let bytes: usize = sidecar.iter().map(Vec::len).sum();
        while !self.held_order.is_empty()
            && (self.held_order.len() >= MAX_HELD_SIDECARS
                || self.held_bytes + bytes > MAX_HELD_SIDECAR_BYTES)
        {
            let evicted = self.held_order.remove(0);
            if let Some((_, evicted_sidecar)) = self.held_sidecars.remove(&evicted) {
                self.held_bytes -= evicted_sidecar.iter().map(Vec::len).sum::<usize>();
            }
        }
        if bytes > MAX_HELD_SIDECAR_BYTES {
            return;
        }
        self.held_bytes += bytes;
        self.held_sidecars.insert(block_ref, (peer, sidecar));
        self.held_order.push(block_ref);
    }

    fn rebuild_slot_floor(&self) {
        let mut floor = vec![Round::MAX; self.context.committee.size()];
        for slot in self.missing_slots.keys() {
            let entry = &mut floor[slot.authority];
            *entry = (*entry).min(slot.round);
        }
        *self.slot_floor.write() = floor;
    }

    fn observe_residency(&self, entry: &PendingMinimal, outcome: &str, local_round: Round) {
        let metrics = &self.context.metrics.node_metrics;
        metrics
            .minimal_block_park_residency_rounds
            .with_label_values(&[outcome])
            .observe(local_round.saturating_sub(entry.parked_at_round) as f64);
    }

    fn update_gauges(&self) {
        let metrics = &self.context.metrics.node_metrics;
        metrics
            .minimal_block_recovery_parked
            .set(self.pending.len() as i64);
        metrics
            .minimal_block_recovery_parked_bytes
            .set(self.total_bytes as i64);
    }

    #[cfg(test)]
    fn assert_invariants(&self) {
        // Every index entry links to a live pending entry and vice versa, and byte
        // accounting reconciles exactly — the properties eviction bugs break first.
        for (slot, dependents) in &self.missing_slots {
            assert!(!dependents.is_empty());
            for dependent in dependents {
                let entry = self.pending.get(dependent).expect("dangling dependent");
                assert!(entry.missing.contains(slot), "index/entry disagree");
            }
        }
        for (block_ref, entry) in &self.pending {
            assert_eq!(self.occupancy.get(&Slot::from(*block_ref)), Some(block_ref));
            for slot in &entry.missing {
                assert!(
                    self.missing_slots
                        .get(slot)
                        .is_some_and(|d| d.contains(block_ref)),
                    "entry slot missing from index"
                );
            }
        }
        assert_eq!(self.occupancy.len(), self.pending.len());
        // Charges are held for dispatched entries too; exact reconciliation is only
        // possible when nothing is in flight.
        if self.in_flight == 0 {
            let total: usize = self.pending.values().map(|e| e.charge).sum();
            assert_eq!(total, self.total_bytes, "byte accounting drifted");
            let mut per_peer = vec![0usize; self.per_peer_bytes.len()];
            for entry in self.pending.values() {
                per_peer[entry.peer] += entry.charge;
            }
            assert_eq!(per_peer, self.per_peer_bytes);
        }
    }
}

/// Where the exact-fetch lane lives: a durable registration with the synchronizer,
/// released by acceptance through any path or by GC. Trait-shaped for tests.
#[async_trait::async_trait]
pub(crate) trait MissingBlockRegistry: Send + Sync + 'static {
    async fn register_missing_block(
        &self,
        block_ref: BlockRef,
    ) -> crate::error::ConsensusResult<()>;
    /// Shares the pending-slot floor with the fetcher so periodic passes can
    /// range-repair parked frontiers. Default no-op for test registries.
    fn install_pending_slot_floor(&self, _floor: PendingSlotFloor) {}
}

#[async_trait::async_trait]
impl MissingBlockRegistry for crate::synchronizer::SynchronizerHandle {
    async fn register_missing_block(
        &self,
        block_ref: BlockRef,
    ) -> crate::error::ConsensusResult<()> {
        crate::synchronizer::SynchronizerHandle::register_missing_block(self, block_ref).await
    }

    fn install_pending_slot_floor(&self, floor: PendingSlotFloor) {
        crate::synchronizer::SynchronizerHandle::install_pending_slot_floor(self, floor);
    }
}

/// The reconstruction worker: drains effects from the acceptance/GC hooks and does
/// everything that must not run under a DagState or pending_reconstructions lock — inflation,
/// submission through the normal verification pipeline, sidecar delivery, and
/// exact-lane registration for entries slot-waiting can no longer help.
///
/// Lock discipline: this task takes the DagState READ guard only for inflation and
/// drops it before touching the pending-reconstructions lock, and never holds the pending-reconstructions lock
/// across an await. The acceptance hook takes pending_reconstructions under the DagState WRITE
/// guard; keeping this task's order opposite-free is what makes that safe.
pub(crate) async fn run_reconstruction_worker<S: crate::network::ValidatorNetworkService>(
    context: Arc<Context>,
    block_inflater: Arc<crate::block_inflater::BlockInflater>,
    dag_state: std::sync::Weak<RwLock<crate::dag_state::DagState>>,
    registry: Arc<dyn MissingBlockRegistry>,
    service: std::sync::Weak<S>,
    pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    mut effects: tokio::sync::mpsc::UnboundedReceiver<AcceptanceEffects>,
) {
    while let Some(batch) = effects.recv().await {
        for (peer, anchor, sidecar) in batch.deliverable_sidecars {
            let Some(service) = service.upgrade() else {
                return;
            };
            let _ = service
                .handle_excluded_ancestors(peer, anchor, sidecar)
                .await;
        }
        for block_ref in batch.frontier_dead {
            if registry.register_missing_block(block_ref).await.is_err() {
                return;
            }
        }
        for entry in batch.ready {
            let (peer, charge) = (entry.peer, entry.charge);
            let inflated = {
                let Some(dag_state) = dag_state.upgrade() else {
                    return;
                };
                let guard = dag_state.read();
                block_inflater.inflate(&entry.minimal, entry.peer, &guard)
            };
            let metrics = &context.metrics.node_metrics;
            match inflated {
                Ok((_signed, serialized)) => {
                    let Some(service) = service.upgrade() else {
                        return;
                    };
                    let block = crate::network::ExtendedSerializedBlock {
                        block: serialized,
                        excluded_ancestors: entry.excluded_ancestors,
                        minimal: None,
                    };
                    if let Err(error) = service.handle_send_block(entry.peer, block).await {
                        // Commit-lag admission control: deliberate backpressure. The
                        // block reaches us again through commit sync; nothing to hold.
                        tracing::debug!(
                            "Reconstructed minimal block {} rejected: {error}",
                            entry.claimed_ref
                        );
                        metrics
                            .minimal_block_quota_drops
                            .with_label_values(&["reconstruction_rejected"])
                            .inc();
                    }
                }
                Err(crate::minimal_block::InflateError::NeedFullBlock { .. }) => {
                    // The frontier filled with different blocks than the sender used
                    // (equivocation) or ambiguity exceeded the search budget: only the
                    // exact block can finish this. Rare by measurement; bounded lane.
                    metrics
                        .minimal_block_quota_drops
                        .with_label_values(&["reinflation_failed"])
                        .inc();
                    if registry
                        .register_missing_block(entry.claimed_ref)
                        .await
                        .is_err()
                    {
                        return;
                    }
                    pending_reconstructions.lock().hold_sidecar(
                        entry.claimed_ref,
                        entry.peer,
                        entry.excluded_ancestors,
                    );
                }
                Err(crate::minimal_block::InflateError::Malformed(error)) => {
                    // fallthrough to shared release below
                    // Structurally validated at receipt, so this indicates a peer
                    // fault surfaced late; drop with a metric.
                    tracing::debug!(
                        "Parked minimal block {} malformed at reconstruction: {error}",
                        entry.claimed_ref
                    );
                    metrics
                        .minimal_block_quota_drops
                        .with_label_values(&["malformed_at_reconstruction"])
                        .inc();
                }
            }
            pending_reconstructions.lock().release(peer, charge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus_types::block::BlockDigest;

    fn pending_reconstructions() -> PendingReconstructions {
        let (context, _) = Context::new_for_test(4);
        PendingReconstructions::new(Arc::new(context))
    }

    fn r(round: Round, author: u32, digest_byte: u8) -> BlockRef {
        let mut digest = [0u8; 32];
        digest[0] = digest_byte;
        BlockRef::new(
            round,
            AuthorityIndex::new_for_test(author),
            BlockDigest(digest),
        )
    }

    fn slot(round: Round, author: u32) -> Slot {
        Slot::new(round, AuthorityIndex::new_for_test(author))
    }

    fn admit(
        p: &mut PendingReconstructions,
        claimed: BlockRef,
        missing: Vec<Slot>,
    ) -> Result<(), AdmitRefusal> {
        let peer = claimed.author;
        p.try_admit(
            claimed,
            Bytes::from(vec![1u8; 64]),
            vec![],
            peer,
            missing,
            0,
            claimed.round,
        )
    }

    /// Accept → decrement → cascade: P waits on {S1,S2}; Q waits on P's claimed
    /// slot. Duplicate acceptances emit nothing; indexes stay exact throughout.
    #[test]
    fn slot_index_cascade() {
        let mut p = pending_reconstructions();
        let p_ref = r(10, 1, 1);
        let q_ref = r(11, 2, 2);
        admit(&mut p, p_ref, vec![slot(9, 2), slot(9, 3)]).unwrap();
        admit(&mut p, q_ref, vec![slot(10, 1)]).unwrap();
        p.assert_invariants();

        // First slot fills: P not ready yet, nothing emitted.
        let effects = p.on_blocks_accepted(&[r(9, 2, 7)]);
        assert!(effects.ready.is_empty());
        p.assert_invariants();

        // Duplicate acceptance at the same slot: nothing.
        let effects = p.on_blocks_accepted(&[r(9, 2, 8)]);
        assert!(effects.ready.is_empty());

        // Second slot fills: P ready, exactly once. Its charge is held until the
        // worker reaches a terminal state.
        let effects = p.on_blocks_accepted(&[r(9, 3, 9)]);
        assert_eq!(effects.ready.len(), 1);
        assert_eq!(effects.ready[0].claimed_ref, p_ref);
        assert!(p.total_bytes > 0, "in-flight charge must stay accounted");
        p.release(effects.ready[0].peer, effects.ready[0].charge);
        p.assert_invariants();

        // P's reconstructed block is accepted: Q cascades.
        let effects = p.on_blocks_accepted(&[p_ref]);
        assert_eq!(effects.ready.len(), 1);
        assert_eq!(effects.ready[0].claimed_ref, q_ref);
        p.release(effects.ready[0].peer, effects.ready[0].charge);
        p.assert_invariants();
        assert_eq!(p.total_bytes, 0, "all charges returned after release");
    }

    /// Window edges are exact, dead frontier slots refuse with their own reason,
    /// and slot occupancy refuses a DIFFERENT ref but is idempotent for the same.
    #[test]
    fn admission_window_occupancy_and_bounds() {
        let mut p = pending_reconstructions();
        let gc = 100;
        let width = window_width(&p.context);
        let admit_at = |p: &mut PendingReconstructions, claimed: BlockRef, missing: Vec<Slot>| {
            p.try_admit(
                claimed,
                Bytes::from(vec![1u8; 64]),
                vec![],
                claimed.author,
                missing,
                gc,
                claimed.round,
            )
        };

        assert_eq!(
            admit_at(&mut p, r(gc, 1, 1), vec![]),
            Err(AdmitRefusal::OutsideWindow),
            "at gc_round is out"
        );
        // The top anchors to the local frontier, which the test helper sets to the
        // claimed round: a claim more than one gc_depth past the frontier is out.
        let frontier = gc + 40;
        let too_far = frontier + width + 1;
        assert_eq!(
            p.try_admit(
                r(too_far, 1, 1),
                Bytes::from(vec![1u8; 64]),
                vec![],
                AuthorityIndex::new_for_test(1),
                vec![],
                gc,
                frontier,
            ),
            Err(AdmitRefusal::OutsideWindow),
            "one past the frontier window is out"
        );
        assert!(
            p.try_admit(
                r(frontier + width, 1, 1),
                Bytes::from(vec![1u8; 64]),
                vec![],
                AuthorityIndex::new_for_test(1),
                vec![slot(gc + 1, 2)],
                gc,
                frontier,
            )
            .is_ok(),
            "window top is in"
        );

        // Dead frontier slot: a wait that can never end.
        assert_eq!(
            admit_at(&mut p, r(gc + 2, 2, 1), vec![slot(gc, 3)]),
            Err(AdmitRefusal::DeadFrontierSlot)
        );

        // Same slot, different digest: refused. Same ref again: idempotent success.
        let first = r(gc + 5, 3, 1);
        assert!(admit_at(&mut p, first, vec![slot(gc + 4, 1)]).is_ok());
        assert_eq!(
            admit_at(&mut p, r(gc + 5, 3, 9), vec![slot(gc + 4, 1)]),
            Err(AdmitRefusal::SlotOccupied)
        );
        assert!(admit_at(&mut p, first, vec![slot(gc + 4, 1)]).is_ok());
        p.assert_invariants();
        // The idempotent re-admit must not double-charge.
        assert_eq!(p.per_peer_bytes[first.author], 64);
    }

    /// Byte quotas refuse at admission; refusal holds the sidecar; GC obsoletes
    /// entries below it and reports frontier-dead entries for exact-lane routing.
    #[test]
    fn quotas_sidecars_and_gc() {
        let mut p = pending_reconstructions();
        // Per-peer quota binds long before the aggregate cap.
        let big = Bytes::from(vec![0u8; MAX_PARKED_BYTES_PER_PEER - 1]);
        let author = AuthorityIndex::new_for_test(1);
        assert!(
            p.try_admit(r(10, 1, 1), big, vec![], author, vec![slot(9, 2)], 0, 10)
                .is_ok()
        );
        let refusal = p.try_admit(
            r(11, 1, 2),
            Bytes::from(vec![0u8; 64]),
            vec![vec![7u8; 8]],
            author,
            vec![slot(9, 2)],
            0,
            11,
        );
        assert_eq!(refusal, Err(AdmitRefusal::PeerBytes));
        // The refused claim's sidecar is held, and delivered on acceptance.
        let effects = p.on_blocks_accepted(&[r(11, 1, 2)]);
        assert_eq!(effects.deliverable_sidecars.len(), 1);
        assert_eq!(effects.deliverable_sidecars[0].2, vec![vec![7u8; 8]]);

        // An entry whose frontier slot crosses GC is reported for the exact lane.
        admit(&mut p, r(30, 2, 1), vec![slot(20, 3)]).unwrap();
        let frontier_dead = p.on_gc(25, 31);
        assert_eq!(frontier_dead, vec![r(30, 2, 1)]);
        // An entry whose claimed round crosses GC is silently obsolete.
        admit(&mut p, r(40, 3, 1), vec![slot(39, 1)]).unwrap();
        assert!(p.on_gc(45, 46).is_empty());
        assert!(p.pending.is_empty());
        p.assert_invariants();
        // Slot floor is clear once nothing is pending.
        assert!(p.slot_floor.read().iter().all(|&r| r == Round::MAX));
    }

    /// The slot floor tracks the lowest pending slot per authority — the value the
    /// periodic scheduler folds into fetch_after_rounds.
    #[test]
    fn slot_floor_tracks_lowest_pending() {
        let mut p = pending_reconstructions();
        admit(&mut p, r(20, 1, 1), vec![slot(18, 2), slot(19, 3)]).unwrap();
        admit(&mut p, r(21, 2, 1), vec![slot(15, 3)]).unwrap();
        {
            let floor = p.slot_floor.read();
            assert_eq!(floor[2], 18);
            assert_eq!(floor[3], 15);
            assert_eq!(floor[1], Round::MAX);
        }
        p.on_blocks_accepted(&[r(15, 3, 5)]);
        assert_eq!(p.slot_floor.read()[3], 19);
    }
}
