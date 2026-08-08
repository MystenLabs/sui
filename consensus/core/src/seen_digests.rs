// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Slot → digest of every block this node has seen, by any path.
//!
//! Reconstructing a minimal block needs its ancestors' 32-byte digests, not the
//! ancestor blocks themselves: a digest is only an identifier, so it can be
//! recorded the moment a block is seen, well before that block is accepted.
//! Every arrival path feeds this map — subscription, synchronizer fetch, commit
//! sync, and own proposals — because a node that has fallen behind receives most
//! of its blocks through fetch and commit sync, exactly the paths that would
//! otherwise contribute nothing to reconstruction.
//!
//! # Never on the critical path of sync
//!
//! This map is written from the fetch and commit-sync paths, so it must never
//! make them wait. Three properties keep that true:
//!
//! - Writes take one sharded lock and do one map insert. Shards are keyed by
//!   authority, so the 128 concurrent arrival streams do not serialise.
//! - `observe_all` returns the slots that became resolvable and does all of its
//!   dispatching decisions *after* releasing the lock. Nothing is ever invoked
//!   while a shard is held, and no other consensus lock is taken while holding
//!   one, so this map can never participate in a lock cycle.
//! - Waking parked reconstructions is the caller's job, done off-thread. A
//!   dropped wakeup costs latency, never correctness: the stale-dependent sweep
//!   fetches the full block regardless.
//!
//! # Trust
//!
//! Digests recorded here are not trusted. A wrong one produces a reconstruction
//! whose bytes do not hash to the sender's claimed digest, which fails closed to
//! the fetch path. Two different digests at one slot mark it ambiguous rather
//! than letting either win. Callers must still only record refs whose author is
//! bound to the authenticated peer, or refs that have already been verified:
//! otherwise one Byzantine fetch peer could mark slots it does not author
//! ambiguous and deny reconstruction of another authority's blocks.
//!
//! # Bounds
//!
//! GC drops every slot at or below `gc_round`, since a block still referencing
//! one is itself obsolete. GC is only a lower bound and stalls when commits
//! stall, so each shard also caps its own size: past the cap, new slots are
//! refused rather than evicting live ones, and reconstruction falls back to
//! fetching. Refusing degrades compression; evicting would corrupt lookups that
//! parked blocks are waiting on.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::RwLock;

use crate::block::Slot;
use crate::context::Context;

/// Shards are indexed by authority, the natural partition: each arrival stream
/// carries one author's blocks, so writers spread across shards without tuning.
const NUM_SHARDS: usize = 16;

/// Headroom over the slots live in the GC window (committee × gc_depth) before a
/// shard stops admitting. Generous, because refusing costs compression, while the
/// cap exists only to bound a commit stall.
const CAPACITY_HEADROOM: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeenSlot {
    Unique(BlockDigest),
    /// Two distinct digests seen at one slot: the author equivocated, or a peer
    /// lied. Resolution falls back to the accepted DAG or an explicit fetch.
    Ambiguous,
}

pub(crate) struct SeenDigests {
    shards: Vec<RwLock<BTreeMap<Slot, SeenSlot>>>,
    /// Where newly-resolved slots go so parked reconstructions can be woken.
    /// Held here rather than at each call site so that no arrival path can record
    /// identity and silently fail to wake its dependents -- recording without
    /// waking would be a regression, since `observe_all` reports a slot as newly
    /// resolved exactly once. Bound late: the reconstruction worker outlives this
    /// map's construction.
    wakeup: OnceLock<tokio::sync::mpsc::Sender<Vec<Slot>>>,
    /// Per-shard admission cap, not a global one: a global counter would need a
    /// shared atomic on every write, reintroducing the contention sharding removes.
    capacity_per_shard: usize,
    context: Arc<Context>,
}

impl SeenDigests {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let live_slots =
            context.committee.size() * context.protocol_config.gc_depth().max(1) as usize;
        let capacity_per_shard =
            (live_slots.saturating_mul(CAPACITY_HEADROOM) / NUM_SHARDS).max(1024);
        Self {
            shards: (0..NUM_SHARDS)
                .map(|_| RwLock::new(BTreeMap::new()))
                .collect(),
            capacity_per_shard,
            wakeup: OnceLock::new(),
            context,
        }
    }

    /// Binds the reconstruction worker's wakeup channel. Called once at startup.
    pub(crate) fn set_wakeup(&self, tx: tokio::sync::mpsc::Sender<Vec<Slot>>) {
        let _ = self.wakeup.set(tx);
    }

    fn shard(&self, authority: AuthorityIndex) -> &RwLock<BTreeMap<Slot, SeenSlot>> {
        &self.shards[authority.value() % NUM_SHARDS]
    }

    /// Records what each ref says about its own slot, returning the slots that
    /// newly resolved to a unique digest — the ones that can unblock a parked
    /// reconstruction. The caller dispatches those wakeups after this returns;
    /// no lock is held once it does.
    ///
    /// Only a block's *own* ref may be passed. Recording the ancestors a block
    /// claims would let its author write into slots it does not own.
    pub(crate) fn observe_all(&self, block_refs: impl IntoIterator<Item = BlockRef>) -> Vec<Slot> {
        use std::collections::btree_map::Entry;
        let mut resolved = Vec::new();
        let mut refused = 0u64;
        for block_ref in block_refs {
            let slot = Slot::from(block_ref);
            let mut shard = self.shard(block_ref.author).write();
            // Admission is checked before taking the entry: past the cap, refuse
            // new slots rather than evicting ones parked blocks may be waiting on.
            if shard.len() >= self.capacity_per_shard && !shard.contains_key(&slot) {
                refused += 1;
                continue;
            }
            match shard.entry(slot) {
                Entry::Vacant(vacant) => {
                    vacant.insert(SeenSlot::Unique(block_ref.digest));
                    resolved.push(slot);
                }
                Entry::Occupied(mut occupied) => {
                    if occupied.get() != &SeenSlot::Unique(block_ref.digest) {
                        occupied.insert(SeenSlot::Ambiguous);
                    }
                }
            }
        }
        if refused > 0 {
            self.context
                .metrics
                .node_metrics
                .minimal_block_seen_digests_refused
                .inc_by(refused);
        }
        // Every shard lock is released by now. `try_send` never blocks, so a fetch
        // or commit-sync thread recording identity is never made to wait on
        // reconstruction; a full channel drops the wakeup, which costs latency only
        // -- the stale-dependent sweep still recovers the block.
        if !resolved.is_empty() {
            if let Some(tx) = self.wakeup.get() {
                let _ = tx.try_send(resolved.clone());
            }
        }
        resolved
    }

    pub(crate) fn observe(&self, block_ref: BlockRef) -> Vec<Slot> {
        self.observe_all(std::iter::once(block_ref))
    }

    /// The digest at `slot`, if exactly one has been seen.
    pub(crate) fn get(&self, slot: Slot) -> Option<BlockDigest> {
        match self.shard(slot.authority).read().get(&slot) {
            Some(SeenSlot::Unique(digest)) => Some(*digest),
            _ => None,
        }
    }

    /// Drops everything at or below `gc_round`. Slots order by round first, so
    /// each shard evicts with one `split_off` rather than a scan.
    pub(crate) fn gc(&self, gc_round: Round) {
        let floor = Slot::new(gc_round.saturating_add(1), AuthorityIndex::ZERO);
        for shard in &self.shards {
            let mut slots = shard.write();
            if slots.is_empty() {
                continue;
            }
            let keep = slots.split_off(&floor);
            *slots = keep;
        }
        self.context
            .metrics
            .node_metrics
            .minimal_block_seen_digests
            .set(self.len() as i64);
    }

    pub(crate) fn len(&self) -> usize {
        self.shards.iter().map(|shard| shard.read().len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn test_seen() -> SeenDigests {
        let (context, _) = Context::new_for_test(4);
        SeenDigests::new(Arc::new(context))
    }

    #[tokio::test]
    async fn records_by_slot_and_marks_conflicts_ambiguous() {
        let seen = test_seen();
        let author = AuthorityIndex::new_for_test(1);
        let a = BlockRef::new(10, author, BlockDigest::MIN);

        assert_eq!(seen.observe(a), vec![Slot::new(10, author)]);
        assert_eq!(seen.get(Slot::new(10, author)), Some(BlockDigest::MIN));

        // The same ref again is idempotent, and resolves nothing new.
        assert!(seen.observe(a).is_empty());
        assert_eq!(seen.get(Slot::new(10, author)), Some(BlockDigest::MIN));

        // A different digest at one slot poisons it rather than overwriting.
        assert!(
            seen.observe(BlockRef::new(10, author, BlockDigest::MAX))
                .is_empty()
        );
        assert_eq!(seen.get(Slot::new(10, author)), None);
    }

    #[tokio::test]
    async fn gc_drops_slots_at_or_below_the_round() {
        let seen = test_seen();
        let author = AuthorityIndex::new_for_test(2);
        for round in 1..=10u32 {
            seen.observe(BlockRef::new(round, author, BlockDigest::MIN));
        }
        seen.gc(7);
        assert_eq!(seen.len(), 3);
        assert_eq!(seen.get(Slot::new(7, author)), None);
        assert_eq!(seen.get(Slot::new(8, author)), Some(BlockDigest::MIN));
    }

    #[tokio::test]
    async fn authorities_land_in_independent_shards() {
        let seen = test_seen();
        // Distinct authorities at one round must not collide: the map is keyed by
        // slot, and sharding must not merge them.
        for authority in 0..4u32 {
            let author = AuthorityIndex::new_for_test(authority);
            seen.observe(BlockRef::new(5, author, BlockDigest::MIN));
        }
        assert_eq!(seen.len(), 4);
        for authority in 0..4u32 {
            let author = AuthorityIndex::new_for_test(authority);
            assert_eq!(seen.get(Slot::new(5, author)), Some(BlockDigest::MIN));
        }
    }
}
