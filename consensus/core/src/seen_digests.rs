// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Slot to digest for blocks this node has seen but has not accepted.
//!
//! Rebuilding a slim block needs its ancestors' 32-byte digests, not the ancestor
//! blocks: a digest is an identifier, so it can be recorded the moment a block is seen,
//! well before that block is accepted. This closes the arrival-to-acceptance window,
//! during which `DagState` knows nothing about a slot and every block referencing it
//! fails to rebuild.
//!
//! Without it the own-parent rule breaks only the same-author cascade. A block that
//! cannot be rebuilt is dropped, so the slot it claims stays unknown, and every later
//! block from any author that references that slot fails too.
//!
//! # Digests here are hints, not facts
//!
//! Nothing recorded here is trusted. A wrong digest produces a rebuild whose bytes do
//! not hash to the sender's claimed digest, which fails closed to a full fetch. That is
//! what makes it safe to record a digest on a peer's word.
//!
//! Two different digests at one slot mark it ambiguous rather than letting either win.
//! First-wins would let an equivocating author choose the rebuild bytes by controlling
//! arrival order.
//!
//! Callers must record only a block's OWN reference, and only once its author is bound
//! to the authenticated peer. Recording the ancestors a block *claims* would let its
//! author write into slots it does not own, turning ambiguity into a denial of service
//! against other authorities. As it stands an author can only poison its own slots,
//! which denies only itself.
//!
//! # Bounds
//!
//! GC drops every slot at or below `gc_round`: a block still referencing one is itself
//! obsolete. That is only a lower bound and stalls when commits stall, so the map also
//! caps its size. Past the cap new slots are refused rather than evicting live ones --
//! refusing costs compression, while evicting would silently change the answer for a
//! slot something is already resolving against.

use std::{collections::HashMap, sync::Arc};

use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::RwLock;

use crate::{
    block::{Slot, SlotDigest},
    context::Context,
};

/// Headroom over the slots live in the GC window (committee x gc_depth) before the map
/// stops admitting. Generous, because refusing costs only compression while the cap
/// exists solely to bound a commit stall.
const CAPACITY_HEADROOM: usize = 4;

/// Floor for the cap on small committees, where the computed window is tiny.
const MIN_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeenSlot {
    Unique(BlockDigest),
    /// Two distinct digests seen at one slot: the author equivocated, or a peer lied.
    /// Terminal until GC.
    Ambiguous,
}

struct State {
    slots: HashMap<Slot, SeenSlot>,
    /// Held beside the map so an insert can reject a slot the last GC already swept.
    /// Without it a reference arriving late -- or racing a GC -- reinserts below the
    /// floor and lingers until some later GC happens to pass it.
    gc_round: Round,
}

pub(crate) struct SeenDigests {
    state: RwLock<State>,
    capacity: usize,
    context: Arc<Context>,
}

impl SeenDigests {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let live_slots =
            context.committee.size() * context.protocol_config.gc_depth().max(1) as usize;
        let capacity = live_slots
            .saturating_mul(CAPACITY_HEADROOM)
            .max(MIN_CAPACITY);
        Self {
            state: RwLock::new(State {
                slots: HashMap::new(),
                gc_round: 0,
            }),
            capacity,
            context,
        }
    }

    /// Records `block_ref` as seen, and advances the GC floor to `gc_round` first.
    ///
    /// Only ever the block's own reference: see the trust note above.
    pub(crate) fn observe(&self, block_ref: BlockRef, gc_round: Round) {
        let mut state = self.state.write();
        if gc_round > state.gc_round {
            state.gc_round = gc_round;
            state.slots.retain(|slot, _| slot.round > gc_round);
        }
        if block_ref.round <= state.gc_round {
            return;
        }
        let slot = Slot::from(block_ref);
        match state.slots.get(&slot).copied() {
            Some(SeenSlot::Ambiguous) => return,
            Some(SeenSlot::Unique(seen)) => {
                if seen != block_ref.digest {
                    state.slots.insert(slot, SeenSlot::Ambiguous);
                }
                return;
            }
            None => {}
        }
        if state.slots.len() >= self.capacity {
            self.context
                .metrics
                .node_metrics
                .slim_block_seen_digests_refused
                .inc();
            return;
        }
        state.slots.insert(slot, SeenSlot::Unique(block_ref.digest));
        let len = state.slots.len();
        drop(state);
        self.context
            .metrics
            .node_metrics
            .slim_block_seen_digests
            .set(len as i64);
    }

    /// What the wire has claimed for `slot`. `Absent` means nothing has been seen, not
    /// that nothing exists.
    pub(crate) fn digest_at_slot(&self, slot: Slot) -> SlotDigest {
        match self.state.read().slots.get(&slot) {
            Some(SeenSlot::Unique(digest)) => SlotDigest::Unique(*digest),
            Some(SeenSlot::Ambiguous) => SlotDigest::Equivocated,
            None => SlotDigest::Absent,
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.state.read().slots.len()
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::AuthorityIndex;

    use super::*;
    use crate::context::Context;

    fn seen(committee_size: usize) -> SeenDigests {
        let (context, _keys) = Context::new_for_test(committee_size);
        SeenDigests::new(Arc::new(context))
    }

    fn block_ref(round: Round, author: u32, byte: u8) -> BlockRef {
        BlockRef::new(
            round,
            AuthorityIndex::new_for_test(author),
            BlockDigest([byte; 32]),
        )
    }

    /// A slot nothing has been seen at is `Absent`, not an error and not a guess.
    #[tokio::test]
    async fn an_unseen_slot_is_absent() {
        let map = seen(4);
        assert_eq!(
            map.digest_at_slot(Slot::from(block_ref(5, 1, 1))),
            SlotDigest::Absent
        );
    }

    /// The property this exists for: a digest is recorded on sight, so a slot resolves
    /// before its block has been accepted anywhere.
    #[tokio::test]
    async fn a_seen_slot_resolves_to_its_digest() {
        let map = seen(4);
        let r = block_ref(5, 1, 7);
        map.observe(r, 0);
        assert_eq!(
            map.digest_at_slot(Slot::from(r)),
            SlotDigest::Unique(r.digest)
        );
    }

    /// Two digests at one slot is equivocation or a lie. Neither wins -- first-wins
    /// would let the author pick the rebuild bytes by controlling arrival order -- and
    /// the verdict is terminal, so a later repeat of either does not undo it.
    #[tokio::test]
    async fn conflicting_digests_are_ambiguous_and_stay_ambiguous() {
        let map = seen(4);
        let first = block_ref(5, 1, 1);
        let second = block_ref(5, 1, 2);
        map.observe(first, 0);
        map.observe(second, 0);
        assert_eq!(
            map.digest_at_slot(Slot::from(first)),
            SlotDigest::Equivocated
        );

        map.observe(first, 0);
        assert_eq!(
            map.digest_at_slot(Slot::from(first)),
            SlotDigest::Equivocated,
            "re-observing the first digest must not resolve the ambiguity"
        );
    }

    /// Repeating the same digest is a no-op rather than a conflict.
    #[tokio::test]
    async fn repeating_a_digest_is_not_a_conflict() {
        let map = seen(4);
        let r = block_ref(5, 1, 3);
        map.observe(r, 0);
        map.observe(r, 0);
        assert_eq!(
            map.digest_at_slot(Slot::from(r)),
            SlotDigest::Unique(r.digest)
        );
        assert_eq!(map.len(), 1);
    }

    /// GC drops the window that is gone, and the floor it leaves behind rejects a
    /// reference that arrives late for a swept round -- otherwise that slot would sit
    /// below the floor until some later GC happened to pass it.
    #[tokio::test]
    async fn gc_drops_swept_rounds_and_the_floor_holds() {
        let map = seen(4);
        map.observe(block_ref(5, 1, 1), 0);
        map.observe(block_ref(9, 2, 2), 0);
        assert_eq!(map.len(), 2);

        // Advancing the floor past round 5 sweeps it and keeps round 9.
        map.observe(block_ref(10, 3, 3), 5);
        assert_eq!(
            map.digest_at_slot(Slot::from(block_ref(5, 1, 1))),
            SlotDigest::Absent
        );
        assert_eq!(
            map.digest_at_slot(Slot::from(block_ref(9, 2, 2))),
            SlotDigest::Unique(BlockDigest([2; 32]))
        );

        // A late arrival for a swept round is refused rather than reinserted.
        map.observe(block_ref(4, 0, 4), 5);
        assert_eq!(
            map.digest_at_slot(Slot::from(block_ref(4, 0, 4))),
            SlotDigest::Absent
        );
    }

    /// At capacity the map refuses new slots instead of evicting live ones: refusing
    /// costs compression, evicting would change the answer for a slot already being
    /// resolved against.
    #[tokio::test]
    async fn at_capacity_new_slots_are_refused_not_evicted() {
        let map = seen(4);
        let capacity = map.capacity;
        // A distinct slot per iteration: the round advances every time.
        for i in 0..capacity {
            map.observe(block_ref(i as Round + 1, (i % 4) as u32, i as u8), 0);
        }
        assert_eq!(
            map.len(),
            capacity,
            "the fixture must actually fill the map"
        );

        let fresh = block_ref(capacity as Round + 1_000, 1, 9);
        map.observe(fresh, 0);
        assert_eq!(
            map.len(),
            capacity,
            "the map must not grow past its capacity"
        );
        assert_eq!(
            map.digest_at_slot(Slot::from(fresh)),
            SlotDigest::Absent,
            "the new slot is refused, not admitted by evicting another"
        );
        // An already-known slot still resolves: refusal never removes anything.
        assert_eq!(
            map.digest_at_slot(Slot::from(block_ref(1, 0, 0))),
            SlotDigest::Unique(BlockDigest([0; 32]))
        );
    }
}
