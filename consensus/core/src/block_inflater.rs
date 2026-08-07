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
use consensus_types::block::{BlockDigest, Round};
use parking_lot::Mutex;

use crate::{
    block::{BlockAPI as _, GENESIS_ROUND, SignedBlock, Slot, VerifiedBlock, genesis_blocks},
    context::Context,
    dag_state::DagState,
    minimal_block::{AncestorDigestResolver, InflateError, deserialize_minimal, serialize_minimal},
    pending_reconstructions::PendingReconstructions,
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
    dag_state: &'a DagState,
    /// Stream-claimed slot digests, consulted only when the accepted DAG has no
    /// candidate. A claim-resolved reconstruction verifies against the dependent's
    /// own claimed digest and signature, then enters block_manager, whose ordinary
    /// suspension covers ancestor content that has not been accepted yet — the
    /// full-form pipeline shape. Lock order: DagState is already held (read) by
    /// callers; PendingReconstructions is taken briefly inside, matching the
    /// DagState-then-pending order the acceptance hook establishes.
    claims: Option<&'a Mutex<PendingReconstructions>>,
}

impl AncestorDigestResolver for DagStateResolver<'_> {
    fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
        if slot.round == GENESIS_ROUND {
            return vec![self.genesis_digests[slot.authority.value()]];
        }
        let accepted: Vec<BlockDigest> = self
            .dag_state
            .get_uncommitted_blocks_at_slot(slot)
            .iter()
            .map(|block| block.digest())
            .collect();
        if !accepted.is_empty() {
            return accepted;
        }
        let Some(claims) = self.claims else {
            return accepted;
        };
        claims.lock().unique_claim(slot).into_iter().collect()
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

    /// Encodes a block for the wire, omitting ancestor digests the receiver can
    /// resolve. Ancestors older than the receivers' DAG-cache retention keep explicit
    /// digests — a caught-up receiver cannot be assumed to resolve rounds it no longer
    /// caches.
    pub(crate) fn serialize(
        &self,
        block: &VerifiedBlock,
        dag_state: &DagState,
    ) -> Result<Bytes, bcs::Error> {
        // Omit digests only for slots every reasonably-synced receiver can still
        // resolve: within one GC depth of the block's round. The sender's own cache
        // reaches much further back, but a receiver's GC does not — an omitted slot
        // below the receiver's GC can never fill, and the claim strands until the
        // exact lane rescues it. Older ancestors (a lagging author referenced by a
        // current block, an excluded-ancestor reconnection) carry their 32-byte
        // digest instead and repair through block_manager's ordinary digest fetch.
        let min_omittable_round = block
            .round()
            .saturating_sub(self.context.protocol_config.gc_depth());
        let resolver = DagStateResolver {
            genesis_digests: &self.genesis_digests,
            dag_state,
            claims: None,
        };
        serialize_minimal(block, &resolver, min_omittable_round)
    }

    /// One inflation attempt against current DAG state. `author` is the peer the bytes
    /// arrived from; a block claiming any other author is rejected before DAG access.
    pub(crate) fn inflate(
        &self,
        minimal: &[u8],
        author: AuthorityIndex,
        dag_state: &DagState,
        claims: Option<&Mutex<PendingReconstructions>>,
    ) -> Result<(SignedBlock, Bytes), InflateError> {
        let resolver = DagStateResolver {
            genesis_digests: &self.genesis_digests,
            dag_state,
            claims,
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
        let minimal = inflater.serialize(&block, &dag_state.read()).unwrap();
        let (_signed, serialized) = inflater
            .inflate(&minimal, block.author(), &dag_state.read(), None)
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
        let minimal = inflater.serialize(&block, &sender_dag.read()).unwrap();

        // Receiver: empty DAG, claims only.
        let empty_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let pending = Mutex::new(crate::pending_reconstructions::PendingReconstructions::new(
            context.clone(),
        ));
        for ancestor in &ancestors {
            pending.lock().observe_claim(
                crate::block::Slot::from(ancestor.reference()),
                ancestor.digest(),
            );
        }
        let (_signed, serialized) = inflater
            .inflate(&minimal, block.author(), &empty_dag.read(), Some(&pending))
            .unwrap();
        assert_eq!(&serialized, block.serialized());
    }
}
