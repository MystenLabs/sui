// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Inflates minimal blocks received on the subscription stream back into full serialized
//! `SignedBlock`s using local DAG state, and produces minimal encodings on the send path.
//!
//! Inflation is synchronous and never waits: a block whose ancestors are not yet accepted
//! locally is reported as un-inflatable so the caller can drop it from the stream and
//! hand it to a recovery task that waits on the missing slot off the critical path
//! (see `minimal_block_receive.rs`).

use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, Round};

use crate::{
    block::{BlockAPI as _, GENESIS_ROUND, SignedBlock, Slot, VerifiedBlock, genesis_blocks},
    context::Context,
    dag_state::DagState,
    minimal_block::{AncestorDigestResolver, InflateError, deserialize_minimal, serialize_minimal},
};

/// Minimal-block codec bound to the committee, usable on both the send side (digest
/// omission decisions) and the receive side (re-inflation). DAG state is passed per
/// call: callers hold it under the ownership discipline of their own context (strong
/// in components, weak in spawned tasks), and the codec stays ownership-free.
pub(crate) struct BlockInflater {
    context: Arc<Context>,
    // Genesis digests by authority index: deterministic from the committee, so round-0
    // refs never depend on cache contents and are never treated as missing.
    genesis_digests: Vec<BlockDigest>,
}

struct DagStateResolver<'a> {
    genesis_digests: &'a [BlockDigest],
    dag_state: &'a DagState,
}

impl AncestorDigestResolver for DagStateResolver<'_> {
    fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
        if slot.round == GENESIS_ROUND {
            return vec![self.genesis_digests[slot.authority.value()]];
        }
        // Accepted DAG candidates only: inflation success therefore implies the
        // block's causal history is locally complete, so it can be accepted at once.
        self.dag_state
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
    ) -> Result<(SignedBlock, Bytes), InflateError> {
        let resolver = DagStateResolver {
            genesis_digests: &self.genesis_digests,
            dag_state,
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
            .inflate(&minimal, block.author(), &dag_state.read())
            .unwrap();
        assert_eq!(&serialized, block.serialized());
    }
}
