// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Inflates minimal blocks received on the subscription stream back into full serialized
//! `SignedBlock`s using local DAG state, and produces minimal encodings on the send path.
//!
//! Inflation is synchronous and never waits: a block whose ancestor digests cannot be
//! resolved right now is reported as un-inflatable so the caller can drop it from the
//! stream and park it (see `pending_reconstructions.rs`). Once a claim or an acceptance
//! resolves the missing slots, the reconstructed block re-enters the normal receive
//! pipeline — verification, then block_manager acceptance — like any full block.

use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::BlockDigest;

use crate::{
    block::{BlockAPI as _, GENESIS_ROUND, SignedBlock, Slot, VerifiedBlock, genesis_blocks},
    context::Context,
    dag_state::DagState,
    minimal_block::{AncestorDigestResolver, InflateError, deserialize_minimal, serialize_minimal},
    seen_digests::SeenDigests,
};

/// Minimal-block codec bound to the committee, usable on both the send side (digest
/// omission decisions) and the receive side (re-inflation).
pub(crate) struct BlockInflater {
    context: Arc<Context>,
    // Genesis digests by authority index: deterministic from the committee, so round-0
    // refs never depend on cache contents and are never treated as missing.
    genesis_digests: Vec<BlockDigest>,
}

struct DagStateResolver<'a> {
    genesis_digests: &'a [BlockDigest],
    /// Optional: callers that cannot afford to hold the DagState guard across an
    /// inflation pass `None` and resolve from `seen` alone.
    dag_state: Option<&'a DagState>,
    /// Digests seen at a slot by any path. Since identity is recorded on arrival
    /// this holds essentially the whole live window, so it answers the common case
    /// on its own, from sharded locks that share nothing with DagState.
    seen: Option<&'a SeenDigests>,
}

impl AncestorDigestResolver for DagStateResolver<'_> {
    /// Seen digests are consulted BEFORE the accepted DAG.
    ///
    /// The order is a throughput decision, not a correctness one: both sources give the
    /// same digest for a slot, because `SeenDigests` records a block's own reference and
    /// yields nothing once a slot is ambiguous. What differs is cost. A DAG lookup runs
    /// under a `DagState` read guard held across the whole inflation, and one inflation
    /// resolves ~100 ancestors; at a few thousand blocks per second per node that is a
    /// torrent of read-lock traffic on the one lock Core must take to WRITE in order to
    /// accept blocks. Readers never block each other, so this never appears as reader
    /// latency — it shows up only as Core doing less work than it should, which is
    /// exactly what the fleet measured against main.
    fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
        if slot.round == GENESIS_ROUND {
            return vec![self.genesis_digests[slot.authority.value()]];
        }
        if let Some(digest) = self.seen.and_then(|seen| seen.get(slot)) {
            return vec![digest];
        }
        let Some(dag_state) = self.dag_state else {
            return vec![];
        };
        dag_state
            .get_uncommitted_blocks_at_slot(slot)
            .iter()
            .map(|block| block.digest())
            .collect()
    }
}

impl BlockInflater {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let genesis_digests = genesis_blocks(&context)
            .iter()
            .map(|block| block.digest())
            .collect();
        Self {
            context,
            genesis_digests,
        }
    }

    /// Test-only convenience joining snapshot and encode in one call; production
    /// always separates them so no DagState guard spans the encoding work.
    #[cfg(test)]
    pub(crate) fn serialize_for_test(
        &self,
        block: &VerifiedBlock,
        dag_state: &DagState,
    ) -> Result<Bytes, bcs::Error> {
        let snapshot = self.ancestor_snapshot(block, dag_state);
        self.serialize_from_snapshot(block, &snapshot)
    }

    /// Snapshot of ancestor-slot candidates for one block, taken under a short
    /// DagState read guard so encoding can run entirely against owned data.
    pub(crate) fn ancestor_snapshot(
        &self,
        block: &VerifiedBlock,
        dag_state: &DagState,
    ) -> std::collections::BTreeMap<Slot, Vec<BlockDigest>> {
        block
            .ancestors()
            .iter()
            .filter(|r| r.round != GENESIS_ROUND)
            .map(|r| {
                let slot = Slot::from(*r);
                (
                    slot,
                    dag_state
                        .get_uncommitted_blocks_at_slot(slot)
                        .iter()
                        .map(|b| b.digest())
                        .collect(),
                )
            })
            .collect()
    }

    /// Encodes against a detached snapshot: no DagState guard is held during the
    /// prost/bcs work. Slots absent from the snapshot conservatively carry their
    /// explicit digest (an ancestor evicted between snapshot and encode costs 32
    /// bytes, never correctness).
    pub(crate) fn serialize_from_snapshot(
        &self,
        block: &VerifiedBlock,
        snapshot: &std::collections::BTreeMap<Slot, Vec<BlockDigest>>,
    ) -> Result<Bytes, bcs::Error> {
        struct SnapshotResolver<'a> {
            genesis_digests: &'a [BlockDigest],
            snapshot: &'a std::collections::BTreeMap<Slot, Vec<BlockDigest>>,
        }
        impl AncestorDigestResolver for SnapshotResolver<'_> {
            fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
                if slot.round == GENESIS_ROUND {
                    return vec![self.genesis_digests[slot.authority.value()]];
                }
                self.snapshot.get(&slot).cloned().unwrap_or_default()
            }
        }
        let min_omittable_round = block
            .round()
            .saturating_sub(self.context.protocol_config.gc_depth());
        let resolver = SnapshotResolver {
            genesis_digests: &self.genesis_digests,
            snapshot,
        };
        serialize_minimal(block, &resolver, min_omittable_round)
    }

    /// One inflation attempt against current DAG state. `author` is the peer the bytes
    /// arrived from; a block claiming any other author is rejected before DAG access.
    pub(crate) fn inflate(
        &self,
        minimal: &[u8],
        author: AuthorityIndex,
        dag_state: Option<&DagState>,
        seen: Option<&SeenDigests>,
    ) -> Result<(SignedBlock, Bytes), InflateError> {
        let resolver = DagStateResolver {
            genesis_digests: &self.genesis_digests,
            dag_state,
            seen,
        };
        deserialize_minimal(minimal, &self.context.committee, author, &resolver)
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::RwLock;

    use super::*;
    use crate::{block::TestBlock, storage::mem_store::MemStore};

    /// Genesis ancestors resolve from the precomputed committee digests, with no DAG
    /// acceptance at all — round-0 refs must never be treated as missing.
    #[tokio::test]
    async fn genesis_ancestors_resolve_without_dag_lookup() {
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let genesis: Vec<_> = genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let inflater = BlockInflater::new(context.clone());
        let block =
            VerifiedBlock::new_for_test(TestBlock::new(1, 0).set_ancestors_raw(genesis).build());
        let minimal = inflater
            .serialize_for_test(&block, &dag_state.read())
            .unwrap();
        let (_signed, serialized) = inflater
            .inflate(&minimal, block.author(), Some(&dag_state.read()), None)
            .unwrap();
        assert_eq!(&serialized, block.serialized());
    }

    /// Omitted ancestor digests resolve from stream claims when the accepted DAG
    /// has no candidate: reconstruction no longer waits on local acceptance.
    #[tokio::test]
    async fn inflate_resolves_omitted_digests_from_claims() {
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let genesis: Vec<_> = genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let mut ancestor_refs = Vec::new();
        let mut ancestors = Vec::new();
        for authority in 0..4u32 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(1, authority)
                    .set_ancestors_raw(genesis.clone())
                    .build(),
            );
            ancestor_refs.push(block.reference());
            sender_dag.write().accept_block(block.clone());
            ancestors.push(block);
        }
        ancestor_refs.sort_by_key(|r| (r.author.value() != 0, r.author));
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(2, 0)
                .set_ancestors_raw(ancestor_refs)
                .build(),
        );
        let inflater = BlockInflater::new(context.clone());
        let minimal = inflater
            .serialize_for_test(&block, &sender_dag.read())
            .unwrap();

        // Receiver: empty DAG, claims only.
        let empty_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let seen = crate::seen_digests::SeenDigests::new(context.clone());
        seen.observe_all(ancestors.iter().map(|a| a.reference()));
        let (_signed, serialized) = inflater
            .inflate(
                &minimal,
                block.author(),
                Some(&empty_dag.read()),
                Some(&seen),
            )
            .unwrap();
        assert_eq!(&serialized, block.serialized());
    }
}
