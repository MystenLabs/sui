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
use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::UnboundedSender;

use crate::{block::Slot, context::Context};

/// Admission window: above `gc_round` (below it the claim is already obsolete) and
/// at most the DAG cache depth above the LOCAL accept frontier. The frontier anchor
/// matters at bootstrap and under commit lag, where the frontier runs arbitrarily
/// far ahead of GC and a gc-anchored top would refuse every live block. The width
/// matters for persistently lagging receivers: a fleet reliably carries nodes
/// hundreds of rounds behind, and a window narrower than that lag turns their whole
/// live stream into refusal/reset churn, where parking quietly costs a few KB per
/// block until commit sync supersedes it. The cache depth is the natural top —
/// beyond it the receiver could not resolve the claim's ancestors anyway — and the
/// entry and byte caps supply the bounds the window itself does not prove.
fn window_width(context: &Context) -> Round {
    (context.parameters.dag_state_cached_rounds as Round)
        .max(2 * context.protocol_config.gc_depth())
}

/// Hard cap on resident entries: four GC windows of one-per-slot occupancy. Honest
/// steady state sits two orders of magnitude below this; reaching it means commits
/// have stalled for minutes, and refusing then is the correct backpressure.
fn max_pending_entries(context: &Context) -> usize {
    context.committee.size() * 4 * context.protocol_config.gc_depth() as usize
}

/// A slot digest claimed by its author's own authenticated stream envelope
/// (structural validation passed, signature unverified). Lets dependents rebuild
/// their ancestor vectors without waiting for local ACCEPTANCE of the ancestor —
/// the dependent's own claimed-digest gate and signature verification validate the
/// choice, and block_manager suspension covers ancestor content that has not
/// arrived. A second, different claim for the same slot marks it ambiguous:
/// resolution then falls back to acceptance or the exact-fetch lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotClaim {
    Unique(BlockDigest),
    Ambiguous,
}

/// Bound on retained claims: one per slot across the admission window.
fn claims_capacity(context: &Context) -> usize {
    context.committee.size() * window_width(context) as usize
}

/// Per-peer resident-byte quota. Honest steady state per peer is a few hundred KB
/// (park rate × residency × payload); this is ~16× that, and it bounds the damage a
/// single author can do by stuffing its own window with maximum-size claims.
pub(crate) const MAX_PARKED_BYTES_PER_PEER: usize = 4 << 20;

/// Node-wide resident-byte cap across all peers — the backstop that keeps worst-case
/// aggregate memory two orders of magnitude below what per-peer quotas alone permit.
const MAX_PARKED_BYTES_TOTAL: usize = 128 << 20;

/// Sidecars held for blocks that were not parked (refused admission or terminal
/// without acceptance). Propagation hints survive the block's refusal and are
/// delivered when it is accepted through any path. Charged against the same
/// per-peer and aggregate byte caps as pending entries, released on delivery
/// or GC through the same path.

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
    /// through `handle_excluded_ancestors` (the anchor precondition now holds). Each
    /// carries the accounting it still holds; the worker releases it after delivery,
    /// so channel-borne bytes stay inside the same caps as resident ones.
    pub(crate) deliverable_sidecars: Vec<(AuthorityIndex, BlockRef, Vec<Vec<u8>>, usize)>,
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
    /// Slot digests claimed by their authors' own stream envelopes. Metadata only:
    /// uncharged, bounded by `claims_capacity`, swept with GC.
    claims: BTreeMap<Slot, SlotClaim>,
    /// When commit-lag shedding first engaged, for the quiesce hysteresis. Cleared
    /// by the first non-lagging receipt.
    lag_shed_since: Option<tokio::time::Instant>,

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
            claims: BTreeMap::new(),
            lag_shed_since: None,

            per_peer_bytes: vec![0; committee_size],
            total_bytes: 0,
            in_flight: 0,
            slot_floor: Arc::new(RwLock::new(vec![Round::MAX; committee_size])),
        }
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
            if claimed_ref.round > window_top {
                self.context
                    .metrics
                    .node_metrics
                    .minimal_block_window_overshoot
                    .observe((claimed_ref.round - window_top) as f64);
            }
            return Some(AdmitRefusal::OutsideWindow);
        }
        if self.pending.len() + self.in_flight >= max_pending_entries(&self.context) {
            return Some(AdmitRefusal::TotalBytes);
        }
        if let Some(deadest) = missing
            .iter()
            .filter(|slot| slot.round <= gc_round)
            .map(|slot| gc_round - slot.round)
            .max()
        {
            self.context
                .metrics
                .node_metrics
                .minimal_block_dead_slot_staleness
                .with_label_values(&["admission"])
                .observe(deadest as f64);
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

    /// The unique claim for `slot`, if any — consulted by the inflater when the
    /// accepted DAG has no candidate.
    pub(crate) fn unique_claim(&self, slot: Slot) -> Option<BlockDigest> {
        match self.claims.get(&slot) {
            Some(SlotClaim::Unique(digest)) => Some(*digest),
            _ => None,
        }
    }

    /// True when every missing slot of the entry carries a unique claim, so its
    /// ancestor vector can be rebuilt without waiting for acceptance.
    fn claim_ready(&self, block_ref: &BlockRef) -> bool {
        let Some(entry) = self.pending.get(block_ref) else {
            return false;
        };
        entry
            .missing
            .iter()
            .all(|slot| matches!(self.claims.get(slot), Some(SlotClaim::Unique(_))))
    }

    /// Records the digest its author's own stream claims for `slot`, returning any
    /// entries that become reconstructable through claims. Conflicts mark the slot
    /// ambiguous rather than replacing: an unverified claim must never win a race.
    pub(crate) fn observe_claim(&mut self, slot: Slot, digest: BlockDigest) -> Vec<ReadyEntry> {
        use std::collections::btree_map::Entry as MapEntry;
        if self.claims.len() >= claims_capacity(&self.context) && !self.claims.contains_key(&slot) {
            return vec![];
        }
        match self.claims.entry(slot) {
            MapEntry::Vacant(vacant) => {
                vacant.insert(SlotClaim::Unique(digest));
            }
            MapEntry::Occupied(mut occupied) => match occupied.get() {
                SlotClaim::Unique(existing) if *existing == digest => return vec![],
                SlotClaim::Unique(_) => {
                    occupied.insert(SlotClaim::Ambiguous);
                    return vec![];
                }
                SlotClaim::Ambiguous => return vec![],
            },
        }
        let Some(dependents) = self.missing_slots.get(&slot) else {
            return vec![];
        };
        let candidates: Vec<BlockRef> = dependents.iter().copied().collect();
        let mut ready = Vec::new();
        for block_ref in candidates {
            if !self.claim_ready(&block_ref) {
                continue;
            }
            let entry = self
                .remove_entry_keeping_charge(&block_ref)
                .expect("entry exists");
            self.in_flight += 1;
            self.observe_residency(&entry, "claim_ready", slot.round);
            ready.push(ReadyEntry {
                claimed_ref: block_ref,
                minimal: entry.minimal,
                excluded_ancestors: entry.excluded_ancestors,
                peer: entry.peer,
                charge: entry.charge,
            });
        }
        if !ready.is_empty() {
            self.rebuild_slot_floor();
            self.update_gauges();
        }
        ready
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
            if let Some(entry) = self.remove_entry_keeping_charge(block_ref) {
                self.in_flight += 1;
                self.observe_residency(&entry, "superseded", block_ref.round);
                effects.deliverable_sidecars.push((
                    entry.peer,
                    *block_ref,
                    entry.excluded_ancestors,
                    entry.charge,
                ));
            }
            if let Some((peer, sidecar)) = self.held_sidecars.remove(block_ref) {
                let bytes: usize = sidecar.iter().map(Vec::len).sum();
                self.in_flight += 1;
                effects
                    .deliverable_sidecars
                    .push((peer, *block_ref, sidecar, bytes));
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
                let empty = entry.missing.is_empty();
                if empty || self.claim_ready(&dependent) {
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
        self.claims.retain(|slot, _| slot.round > gc_round);
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
            if let Some(deadest) = entry
                .missing
                .iter()
                .filter(|slot| slot.round <= gc_round)
                .map(|slot| gc_round - slot.round)
                .max()
            {
                self.context
                    .metrics
                    .node_metrics
                    .minimal_block_dead_slot_staleness
                    .with_label_values(&["gc"])
                    .observe(deadest as f64);
            }
            self.hold_sidecar(*block_ref, entry.peer, entry.excluded_ancestors.clone());
            self.observe_residency(&entry, "frontier_dead", local_round);
        }
        let swept: Vec<BlockRef> = self
            .held_sidecars
            .keys()
            .filter(|r| r.round <= gc_round)
            .copied()
            .collect();
        let mut swept_any = false;
        for block_ref in swept {
            if let Some((peer, sidecar)) = self.held_sidecars.remove(&block_ref) {
                let bytes: usize = sidecar.iter().map(Vec::len).sum();
                self.per_peer_bytes[peer] -= bytes;
                self.total_bytes -= bytes;
                swept_any = true;
            }
        }
        if swept_any {
            self.update_gauges();
        }
        if !obsolete.is_empty() || !frontier_dead.is_empty() {
            self.rebuild_slot_floor();
            self.update_gauges();
        }
        frontier_dead
    }

    /// Quiesce hysteresis: eviction is warranted only when commit lag SUSTAINS —
    /// a frozen node whose GC has stopped and whose parked mass would otherwise pin
    /// the caps for the whole lag. A node that merely flaps across the lag boundary
    /// (transient commit latency on a distant region) must not dump tens of
    /// thousands of healthy in-race parks on a single shed: its GC still runs, so
    /// the caps bound memory without eviction. Sheds always drop the arriving
    /// block; this only gates the eviction of what is already parked.
    pub(crate) fn should_quiesce_on_shed(&mut self) -> bool {
        const QUIESCE_AFTER: std::time::Duration = std::time::Duration::from_secs(20);
        match self.lag_shed_since {
            None => {
                self.lag_shed_since = Some(tokio::time::Instant::now());
                false
            }
            Some(since) => since.elapsed() >= QUIESCE_AFTER,
        }
    }

    /// First non-lagging receipt clears the hysteresis window.
    pub(crate) fn note_not_lagging(&mut self) {
        self.lag_shed_since = None;
    }

    /// Resident-only eviction of every parked entry and held sidecar, releasing
    /// their byte charges. Called when the node enters commit-lag shedding.
    /// Deliberate divergence from full mode (which retains blocks already admitted
    /// to block_manager): an entry admitted before the lag could in principle
    /// complete after catch-up, but retaining it keeps the byte caps pinned for the
    /// whole lag because GC freezes with commits. Bounded memory wins; committed
    /// content is recovered through commit sync regardless. Queued and in-flight
    /// reconstructions are untouched — their charges release at their terminal
    /// transitions, never here. Returns the number of evicted items.
    pub(crate) fn quiesce(&mut self) -> usize {
        if self.pending.is_empty() && self.held_sidecars.is_empty() {
            return 0;
        }
        let refs: Vec<BlockRef> = self.pending.keys().copied().collect();
        let mut evicted = refs.len();
        for block_ref in &refs {
            self.remove_entry(block_ref).expect("entry exists");
        }
        let held: Vec<BlockRef> = self.held_sidecars.keys().copied().collect();
        evicted += held.len();
        for block_ref in held {
            if let Some((peer, sidecar)) = self.held_sidecars.remove(&block_ref) {
                let bytes: usize = sidecar.iter().map(Vec::len).sum();
                self.per_peer_bytes[peer] -= bytes;
                self.total_bytes -= bytes;
            }
        }
        self.rebuild_slot_floor();
        self.update_gauges();
        evicted
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
        // Same caps as pending entries: hints are peer-attributed resident bytes.
        // Refusal is explicit — a dropped hint is an observable event, not a silent
        // eviction — and only cap pressure (which resets the stream for full
        // replay, re-delivering the hints) can cause it.
        if self.per_peer_bytes[peer] + bytes > MAX_PARKED_BYTES_PER_PEER
            || self.total_bytes + bytes > MAX_PARKED_BYTES_TOTAL
        {
            self.context
                .metrics
                .node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["sidecar_dropped"])
                .inc();
            return;
        }
        self.per_peer_bytes[peer] += bytes;
        self.total_bytes += bytes;
        self.held_sidecars.insert(block_ref, (peer, sidecar));
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
            let mut per_peer = vec![0usize; self.per_peer_bytes.len()];
            for entry in self.pending.values() {
                per_peer[entry.peer] += entry.charge;
            }
            for (peer, sidecar) in self.held_sidecars.values() {
                per_peer[*peer] += sidecar.iter().map(Vec::len).sum::<usize>();
            }
            let total: usize = per_peer.iter().sum();
            assert_eq!(total, self.total_bytes, "byte accounting drifted");
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
/// everything that must not run under a DagState or pending lock — inflation,
/// submission through the normal verification pipeline, sidecar delivery, and
/// exact-lane registration.
///
/// Submissions run with bounded concurrency. Each costs a full verification and
/// core dispatch, and at busy tips ready entries arrive faster than one serial
/// await chain can complete them; a serial worker pins in-flight charges until the
/// byte cap turns arrival racing into refusals.
///
/// Lock discipline: DagState READ guards are scoped to inflation and dropped before
/// the pending mutex, and the mutex is never held across an await.
const RECONSTRUCTION_CONCURRENCY: usize = 32;

pub(crate) async fn run_reconstruction_worker<S: crate::network::ValidatorNetworkService>(
    context: Arc<Context>,
    block_inflater: Arc<crate::block_inflater::BlockInflater>,
    dag_state: std::sync::Weak<RwLock<crate::dag_state::DagState>>,
    registry: Arc<dyn MissingBlockRegistry>,
    service: std::sync::Weak<S>,
    pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    mut effects: tokio::sync::mpsc::UnboundedReceiver<AcceptanceEffects>,
) {
    use futures::stream::{FuturesUnordered, StreamExt as _};

    let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
    // Ready entries beyond the concurrency bound queue here: the bound is enforced
    // per entry, not per effects batch, and ready work is dispatched before the
    // sequential sidecar deliveries of the same batch.
    let mut ready_queue: std::collections::VecDeque<ReadyEntry> = Default::default();
    loop {
        while in_flight.len() < RECONSTRUCTION_CONCURRENCY {
            let Some(entry) = ready_queue.pop_front() else {
                break;
            };
            in_flight.push(reconstruct_one(
                context.clone(),
                block_inflater.clone(),
                dag_state.clone(),
                registry.clone(),
                service.clone(),
                pending_reconstructions.clone(),
                entry,
            ));
        }
        tokio::select! {
            batch = effects.recv() => {
                let Some(batch) = batch else { break };
                ready_queue.extend(batch.ready);
                for (peer, anchor, sidecar, charge) in batch.deliverable_sidecars {
                    {
                        let Some(service) = service.upgrade() else { return };
                        let _ = service.handle_excluded_ancestors(peer, anchor, sidecar).await;
                    }
                    pending_reconstructions.lock().release(peer, charge);
                }
                for block_ref in batch.frontier_dead {
                    if registry.register_missing_block(block_ref).await.is_err() {
                        return;
                    }
                }
            }
            Some(alive) = in_flight.next() => {
                if !alive {
                    return;
                }
            }
            else => break,
        }
    }
    // Channel closed: drain queued and in-flight work.
    while in_flight.next().await.is_some() || !ready_queue.is_empty() {
        while in_flight.len() < RECONSTRUCTION_CONCURRENCY {
            let Some(entry) = ready_queue.pop_front() else {
                break;
            };
            in_flight.push(reconstruct_one(
                context.clone(),
                block_inflater.clone(),
                dag_state.clone(),
                registry.clone(),
                service.clone(),
                pending_reconstructions.clone(),
                entry,
            ));
        }
    }
}

/// One reconstruction from ready entry to terminal state; returns false when the
/// node is shutting down (service or DagState gone).
async fn reconstruct_one<S: crate::network::ValidatorNetworkService>(
    context: Arc<Context>,
    block_inflater: Arc<crate::block_inflater::BlockInflater>,
    dag_state: std::sync::Weak<RwLock<crate::dag_state::DagState>>,
    registry: Arc<dyn MissingBlockRegistry>,
    service: std::sync::Weak<S>,
    pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    entry: ReadyEntry,
) -> bool {
    let (peer, charge) = (entry.peer, entry.charge);
    let inflated = {
        let Some(dag_state) = dag_state.upgrade() else {
            return false;
        };
        let guard = dag_state.read();
        block_inflater.inflate(
            &entry.minimal,
            entry.peer,
            &guard,
            Some(&pending_reconstructions),
        )
    };
    let metrics = &context.metrics.node_metrics;
    match inflated {
        Ok((_signed, serialized)) => {
            let Some(service) = service.upgrade() else {
                return false;
            };
            let block = crate::network::ExtendedSerializedBlock {
                block: serialized,
                excluded_ancestors: entry.excluded_ancestors,
                minimal: None,
            };
            if let Err(error) = service.handle_send_block(entry.peer, block).await {
                // Commit-lag admission control: deliberate backpressure. The block
                // reaches us again through commit sync; nothing to hold.
                tracing::debug!(
                    "Reconstructed minimal block {} rejected: {error}",
                    entry.claimed_ref
                );
                metrics
                    .minimal_block_recovery_outcomes
                    .with_label_values(&["reconstruction_rejected"])
                    .inc();
            }
        }
        Err(crate::minimal_block::InflateError::NeedFullBlock { .. }) => {
            // The frontier filled with different blocks than the sender used
            // (equivocation) or ambiguity exceeded the search budget: only the exact
            // block can finish this.
            metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["reinflation_failed"])
                .inc();
            if registry
                .register_missing_block(entry.claimed_ref)
                .await
                .is_err()
            {
                return false;
            }
            // Release before holding: the entry charge still covers these bytes, and
            // holding first double-counts the sidecar against the unified cap —
            // refusing it on capacity that is about to free.
            {
                let mut pending = pending_reconstructions.lock();
                pending.release(peer, charge);
                pending.hold_sidecar(entry.claimed_ref, entry.peer, entry.excluded_ancestors);
            }
            return true;
        }
        Err(crate::minimal_block::InflateError::Malformed(error)) => {
            tracing::debug!(
                "Pending minimal block {} malformed at reconstruction: {error}",
                entry.claimed_ref
            );
            metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["malformed_at_reconstruction"])
                .inc();
        }
    }
    pending_reconstructions.lock().release(peer, charge);
    true
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
        // Under cap pressure the hint cannot be held either: it is dropped with an
        // explicit outcome (the accompanying stream reset re-delivers it in full
        // form), never evicted silently.
        assert!(
            p.on_blocks_accepted(&[r(11, 1, 2)])
                .deliverable_sidecars
                .is_empty()
        );

        // Below the caps, a refused claim's sidecar IS held and delivered on
        // acceptance, charged like any other resident bytes until the worker
        // releases it.
        let other = r(12, 2, 1);
        admit(&mut p, other, vec![slot(11, 3)]).unwrap();
        let refusal = p.try_admit(
            r(12, 2, 9),
            Bytes::from(vec![0u8; 64]),
            vec![vec![9u8; 8]],
            other.author,
            vec![slot(11, 3)],
            0,
            12,
        );
        assert_eq!(refusal, Err(AdmitRefusal::SlotOccupied));
        let effects = p.on_blocks_accepted(&[r(12, 2, 9)]);
        assert_eq!(effects.deliverable_sidecars.len(), 1);
        assert_eq!(effects.deliverable_sidecars[0].2, vec![vec![9u8; 8]]);
        let (peer, _, _, charge) = (
            effects.deliverable_sidecars[0].0,
            (),
            (),
            effects.deliverable_sidecars[0].3,
        );
        p.release(peer, charge);

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

    /// A dispatched entry's charge still covers its sidecar; releasing before
    /// holding is what lets the hint survive a re-inflation failure at the cap.
    #[test]
    fn release_then_hold_survives_cap_pressure() {
        let mut p = pending_reconstructions();
        let author = AuthorityIndex::new_for_test(1);
        let payload = MAX_PARKED_BYTES_PER_PEER - 8;
        assert!(
            p.try_admit(
                r(10, 1, 1),
                Bytes::from(vec![0u8; payload]),
                vec![vec![7u8; 8]],
                author,
                vec![slot(9, 2)],
                0,
                10,
            )
            .is_ok()
        );
        let effects = p.on_blocks_accepted(&[r(9, 2, 5)]);
        assert_eq!(effects.ready.len(), 1);
        let entry = &effects.ready[0];
        // Worker order on NeedFullBlock: release first, then hold — the sidecar
        // fits because the entry's own charge just freed.
        p.release(entry.peer, entry.charge);
        p.hold_sidecar(r(10, 1, 1), author, vec![vec![7u8; 8]]);
        assert_eq!(p.held_sidecars.len(), 1, "hint must survive cap pressure");
        assert_eq!(p.per_peer_bytes[author], 8);
        p.assert_invariants();
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

    /// Quiesce evicts parked and held state, zeroes the books, and leaves the
    /// structure ready for fresh admissions.
    #[tokio::test]
    async fn quiesce_releases_everything() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let mut pending = PendingReconstructions::new(context.clone());
        let peer = context.committee.to_authority_index(1).unwrap();
        let claim_a = BlockRef::new(10, peer, BlockDigest::MIN);
        let claim_b = BlockRef::new(11, peer, BlockDigest::MAX);
        let missing = vec![Slot::new(
            9,
            context.committee.to_authority_index(2).unwrap(),
        )];
        pending
            .try_admit(
                claim_a,
                Bytes::from(vec![1u8; 100]),
                vec![],
                peer,
                missing.clone(),
                0,
                9,
            )
            .unwrap();
        pending
            .try_admit(
                claim_b,
                Bytes::from(vec![2u8; 100]),
                vec![],
                peer,
                missing,
                0,
                10,
            )
            .unwrap();
        pending.hold_sidecar(
            BlockRef::new(12, peer, BlockDigest::MIN),
            peer,
            vec![vec![0u8; 64]],
        );
        assert_eq!(pending.quiesce(), 3);
        pending.assert_invariants();
        assert_eq!(pending.quiesce(), 0);
        // Fresh admissions work after a quiesce.
        pending
            .try_admit(
                BlockRef::new(13, peer, BlockDigest::MIN),
                Bytes::from(vec![3u8; 100]),
                vec![],
                peer,
                vec![Slot::new(12, peer)],
                0,
                12,
            )
            .unwrap();
        pending.assert_invariants();
    }

    /// A claim from the missing author's own envelope readies dependents without
    /// acceptance; a conflicting claim poisons the slot to ambiguous instead.
    #[tokio::test]
    async fn observe_claim_readies_dependents_and_conflicts_poison() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let mut pending = PendingReconstructions::new(context.clone());
        let peer = context.committee.to_authority_index(1).unwrap();
        let author = context.committee.to_authority_index(2).unwrap();
        let claim_ref = BlockRef::new(10, peer, BlockDigest::MIN);
        let missing_slot = Slot::new(9, author);
        pending
            .try_admit(
                claim_ref,
                Bytes::from(vec![1u8; 64]),
                vec![],
                peer,
                vec![missing_slot],
                0,
                9,
            )
            .unwrap();
        let ready = pending.observe_claim(missing_slot, BlockDigest::MAX);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].claimed_ref, claim_ref);
        assert_eq!(pending.unique_claim(missing_slot), Some(BlockDigest::MAX));
        pending.release(peer, ready[0].charge);

        // Conflicting claims mark the slot ambiguous: no unique digest served.
        let other = Slot::new(9, peer);
        assert!(pending.observe_claim(other, BlockDigest::MIN).is_empty());
        assert!(pending.observe_claim(other, BlockDigest::MAX).is_empty());
        assert_eq!(pending.unique_claim(other), None);
        pending.assert_invariants();
    }
}
