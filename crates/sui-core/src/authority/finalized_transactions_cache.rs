// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use parking_lot::RwLock;
use sui_types::digests::TransactionDigest;

/// An advisory set of recently finalized transaction digests, used by the
/// transaction voting path to short-circuit re-voting on already-finalized
/// transactions (`AuthorityPerEpochStore::is_recently_finalized`).
///
/// Workload: a single writer (the consensus commit handler, ~one insert per
/// finalized transaction) and many concurrent readers (the voting path, one
/// `contains` per submitted transaction across the whole RPC/network worker
/// pool). This is the inverse of what a concurrent cache like `moka` optimizes
/// for cheaply: `moka`'s per-insert maintenance dominated the single-threaded
/// handler at high TPS, while its lock-free reads were not the constraint.
///
/// Membership is all the read path needs, so entries never need recency
/// promotion on read — that lets readers take a shared lock and run
/// concurrently. Eviction is generational (FIFO): inserts fill `current`; once
/// it reaches the per-generation capacity, `current` ages into `previous` (the
/// prior `previous` is dropped) and a fresh `current` begins. `contains` checks
/// both generations, so the most recent `generation_capacity` digests are always
/// retained, and at most `2 * generation_capacity` are held at once. FIFO is also
/// a better fit than LRU here: a transaction is finalized once, so being
/// repeatedly voted on should not extend how long it is "recently finalized".
///
/// The cache is advisory — `is_recently_finalized` falls back to the executed-tx
/// check — so the eventual-consistency and bounded-window behavior here is safe.
pub struct FinalizedTransactionsCache {
    inner: RwLock<Generations>,
    /// Age `current` into `previous` once it reaches this many entries.
    generation_capacity: usize,
}

struct Generations {
    current: HashSet<TransactionDigest>,
    previous: HashSet<TransactionDigest>,
}

impl FinalizedTransactionsCache {
    /// `capacity` is the target total number of digests retained; it is split
    /// across two generations so the live set stays within `capacity`.
    pub fn new(capacity: usize) -> Self {
        let generation_capacity = (capacity / 2).max(1);
        Self {
            inner: RwLock::new(Generations {
                current: HashSet::with_capacity(generation_capacity),
                previous: HashSet::new(),
            }),
            generation_capacity,
        }
    }

    pub fn insert(&self, tx_digest: TransactionDigest) {
        // Hold the write lock only for the O(1) in-memory mutation. On rotation we
        // move the aged-out generation *out* (rather than letting `inner.previous =
        // full` drop it in place) so its deallocation happens after the block — i.e.
        // after the write guard is released — keeping it off the readers' critical
        // path.
        let _aged_out = {
            let mut inner = self.inner.write();
            let aged_out = if inner.current.len() >= self.generation_capacity {
                let full = std::mem::replace(
                    &mut inner.current,
                    HashSet::with_capacity(self.generation_capacity),
                );
                Some(std::mem::replace(&mut inner.previous, full))
            } else {
                None
            };
            inner.current.insert(tx_digest);
            aged_out
        };
    }

    pub fn contains(&self, tx_digest: &TransactionDigest) -> bool {
        let inner = self.inner.read();
        inner.current.contains(tx_digest) || inner.previous.contains(tx_digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_contains() {
        let cache = FinalizedTransactionsCache::new(100);
        let a = TransactionDigest::random();
        let b = TransactionDigest::random();
        assert!(!cache.contains(&a));
        cache.insert(a);
        assert!(cache.contains(&a));
        assert!(!cache.contains(&b));
    }

    #[test]
    fn retains_across_one_rotation_then_evicts() {
        // generation_capacity = 1: each generation holds one entry, so an entry
        // survives exactly one rotation before being dropped.
        let cache = FinalizedTransactionsCache::new(2);
        let first = TransactionDigest::random();
        cache.insert(first); // current = {first}

        let second = TransactionDigest::random();
        cache.insert(second); // rotate: previous = {first}, current = {second}
        assert!(cache.contains(&first), "survives one rotation");
        assert!(cache.contains(&second));

        let third = TransactionDigest::random();
        cache.insert(third); // rotate: previous = {second}, current = {third}
        assert!(!cache.contains(&first), "evicted after second rotation");
        assert!(cache.contains(&second));
        assert!(cache.contains(&third));
    }

    #[test]
    fn rotation_boundary_with_larger_generation() {
        // capacity = 100 => generation_capacity = 50; exercises the >= boundary
        // for G > 1 (the single-rotation test above only covers G = 1).
        let cache = FinalizedTransactionsCache::new(100);
        let batch_a: Vec<_> = (0..50).map(|_| TransactionDigest::random()).collect();
        let batch_b: Vec<_> = (0..50).map(|_| TransactionDigest::random()).collect();

        // Fill `current` exactly to capacity; no rotation yet, all of A present.
        for d in &batch_a {
            cache.insert(*d);
        }
        assert!(batch_a.iter().all(|d| cache.contains(d)));

        // The first B insert rotates A into `previous`, where it survives the full
        // generation; afterwards both batches are retained (2 * generation_capacity).
        for d in &batch_b {
            cache.insert(*d);
        }
        assert!(
            batch_a.iter().all(|d| cache.contains(d)),
            "A survives one rotation"
        );
        assert!(batch_b.iter().all(|d| cache.contains(d)));

        // One more insert rotates again and evicts A; B and the new entry remain.
        let extra = TransactionDigest::random();
        cache.insert(extra);
        assert!(
            batch_a.iter().all(|d| !cache.contains(d)),
            "A evicted on next rotation"
        );
        assert!(batch_b.iter().all(|d| cache.contains(d)));
        assert!(cache.contains(&extra));
    }
}
