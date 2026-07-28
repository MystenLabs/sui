// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Inflates minimal blocks received on the subscription stream back into full serialized
//! `SignedBlock`s using local DAG state, and produces minimal encodings on the send path.
//!
//! A missing ancestor on inflation is often a block still in the acceptance pipeline, so
//! inflation retries once after a short delay before the caller falls back to fetching the
//! full block from the sending peer (which authored it — see `minimal_block.rs`).

// TODO(minimal-blocks): remove once wired into the subscribe_blocks path.
#![allow(dead_code)]

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use consensus_types::block::{BlockDigest, Round};
use parking_lot::RwLock;

use crate::{
    block::{GENESIS_ROUND, SignedBlock, Slot, VerifiedBlock, genesis_blocks},
    context::Context,
    dag_state::DagState,
    minimal_block::{
        AncestorDigestResolver, FallbackReason, InflateError, MinimalBlockError,
        deserialize_minimal, serialize_minimal,
    },
};

/// A missing ancestor may be an in-flight block; wait this long before the single retry.
const INFLATE_RETRY_DELAY: Duration = Duration::from_millis(20);

/// Minimal-block codec bound to live DAG state, usable on both the send side (digest
/// omission decisions) and the receive side (re-inflation).
pub(crate) struct BlockInflater {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    // Genesis digests by authority index: deterministic from the committee, so round-0
    // refs never depend on cache contents and are never treated as missing.
    genesis_digests: Vec<BlockDigest>,
}

struct DagStateResolver<'a>(&'a BlockInflater);

impl AncestorDigestResolver for DagStateResolver<'_> {
    fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
        if slot.round == GENESIS_ROUND {
            return vec![self.0.genesis_digests[slot.authority]];
        }
        self.0
            .dag_state
            .read()
            .get_uncommitted_blocks_at_slot(slot)
            .iter()
            .map(|block| block.digest())
            .collect()
    }
}

impl BlockInflater {
    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let genesis_digests = genesis_blocks(&context)
            .iter()
            .map(|block| block.digest())
            .collect();
        Self {
            context,
            dag_state,
            genesis_digests,
        }
    }

    /// Encodes a block for the wire, omitting ancestor digests the receiver can resolve.
    /// Slots at or below `min_omittable_round` keep explicit digests (cache horizon).
    pub(crate) fn serialize(
        &self,
        block: &VerifiedBlock,
        min_omittable_round: Round,
    ) -> Result<Bytes, MinimalBlockError> {
        serialize_minimal(block, &DagStateResolver(self), min_omittable_round)
    }

    /// One inflation attempt against current DAG state.
    pub(crate) fn inflate(&self, minimal: &[u8]) -> Result<(SignedBlock, Bytes), InflateError> {
        deserialize_minimal(minimal, &self.context.committee, &DagStateResolver(self))
    }

    /// Inflation with a single bounded retry on a missing ancestor. Any remaining failure
    /// is the caller's cue to fetch the full block from the sending peer.
    pub(crate) async fn inflate_with_retry(
        &self,
        minimal: &[u8],
    ) -> Result<(SignedBlock, Bytes), InflateError> {
        match self.inflate(minimal) {
            Err(InflateError::NeedFullBlock {
                reason: FallbackReason::MissingAncestor(_),
                ..
            }) => {
                tokio::time::sleep(INFLATE_RETRY_DELAY).await;
                self.inflate(minimal)
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::AuthorityIndex;
    use consensus_types::block::BlockRef;

    use super::*;
    use crate::{block::TestBlock, storage::mem_store::MemStore};

    fn setup(
        committee_size: usize,
    ) -> (
        Arc<Context>,
        Vec<crate::block::VerifiedBlock>,
        BlockInflater,
    ) {
        let (context, _key_pairs) = Context::new_for_test(committee_size);
        let context = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let genesis = genesis_blocks(&context);
        (
            context.clone(),
            genesis,
            BlockInflater::new(context, dag_state),
        )
    }

    /// Builds, accepts, and returns one block per authority at `round`, wired to the
    /// previous `ancestors`.
    fn accept_round(
        inflater: &BlockInflater,
        committee_size: usize,
        round: Round,
        ancestors: &[BlockRef],
    ) -> Vec<BlockRef> {
        (0..committee_size)
            .map(|authority| {
                let mut refs = ancestors.to_vec();
                // Own ancestor first, as the verifier requires.
                refs.sort_by_key(|r| (r.author.value() != authority, r.author));
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, authority as u32)
                        .set_ancestors_raw(refs)
                        .build(),
                );
                let reference = block.reference();
                inflater.dag_state.write().accept_block(block);
                reference
            })
            .collect()
    }

    #[tokio::test]
    async fn inflate_from_live_dag_state() {
        let (_context, genesis, inflater) = setup(4);
        let genesis_refs: Vec<_> = genesis.iter().map(|b| b.reference()).collect();
        let round1 = accept_round(&inflater, 4, 1, &genesis_refs);
        let round2 = accept_round(&inflater, 4, 2, &round1);

        let mut ancestors = round2;
        ancestors.sort_by_key(|r| (r.author.value() != 0, r.author));
        let block =
            VerifiedBlock::new_for_test(TestBlock::new(3, 0).set_ancestors_raw(ancestors).build());

        let minimal = inflater.serialize(&block, 0).unwrap();
        let (_signed, serialized) = inflater.inflate(&minimal).unwrap();
        assert_eq!(&serialized, block.serialized());
        assert!(minimal.len() < block.serialized().len());
    }

    #[tokio::test]
    async fn genesis_ancestors_resolve_without_dag_lookup() {
        let (_context, genesis, inflater) = setup(4);
        let genesis_refs: Vec<_> = genesis.iter().map(|b| b.reference()).collect();
        // Note: genesis blocks are never accepted into recent_blocks here — resolution
        // must come from the precomputed genesis digests.
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(1, 0).set_ancestors_raw(genesis_refs).build(),
        );
        let minimal = inflater.serialize(&block, 0).unwrap();
        let (_signed, serialized) = inflater.inflate(&minimal).unwrap();
        assert_eq!(&serialized, block.serialized());
    }

    #[tokio::test]
    async fn missing_ancestor_fails_then_succeeds_after_acceptance() {
        let (_context, genesis, inflater) = setup(4);
        let genesis_refs: Vec<_> = genesis.iter().map(|b| b.reference()).collect();
        let round1 = accept_round(&inflater, 4, 1, &genesis_refs);

        // A round-2 block from authority 3 that the receiver hasn't accepted yet.
        let mut refs = round1.clone();
        refs.sort_by_key(|r| (r.author.value() != 3, r.author));
        let late_block =
            VerifiedBlock::new_for_test(TestBlock::new(2, 3).set_ancestors_raw(refs).build());

        let mut ancestors = round1;
        ancestors.sort_by_key(|r| (r.author.value() != 0, r.author));
        ancestors[3] = late_block.reference();
        let block =
            VerifiedBlock::new_for_test(TestBlock::new(3, 0).set_ancestors_raw(ancestors).build());

        // Sender knows the late block (it links to it), receiver doesn't yet.
        inflater.dag_state.write().accept_block(late_block.clone());
        let minimal = inflater.serialize(&block, 0).unwrap();

        let receiver = {
            let (context, _key_pairs) = Context::new_for_test(4);
            let context = Arc::new(context);
            let dag_state = Arc::new(RwLock::new(DagState::new(
                context.clone(),
                Arc::new(MemStore::new()),
            )));
            BlockInflater::new(context, dag_state)
        };
        let genesis_refs: Vec<_> = genesis.iter().map(|b| b.reference()).collect();
        accept_round(&receiver, 4, 1, &genesis_refs);

        match receiver.inflate_with_retry(&minimal).await.map(|_| ()) {
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                assert_eq!(block_ref, block.reference());
                assert_eq!(
                    reason,
                    FallbackReason::MissingAncestor(Slot::from(late_block.reference()))
                );
            }
            other => panic!("expected MissingAncestor fallback, got {other:?}"),
        }

        // Once the in-flight ancestor lands, the same bytes inflate cleanly.
        receiver.dag_state.write().accept_block(late_block);
        let (_signed, serialized) = receiver.inflate(&minimal).unwrap();
        assert_eq!(&serialized, block.serialized());
    }

    #[tokio::test]
    async fn equivocating_slot_gets_digest_hint_from_sender() {
        let (_context, genesis, inflater) = setup(4);
        let genesis_refs: Vec<_> = genesis.iter().map(|b| b.reference()).collect();
        let round1 = accept_round(&inflater, 4, 1, &genesis_refs);

        // Authority 2 equivocates at round 1; the sender knows both blocks.
        let mut refs = genesis_refs.clone();
        refs.sort_by_key(|r| (r.author != AuthorityIndex::new_for_test(2), r.author));
        let equivocation = VerifiedBlock::new_for_test(
            TestBlock::new(1, 2)
                .set_ancestors_raw(refs)
                .set_timestamp_ms(9999)
                .build(),
        );
        inflater.dag_state.write().accept_block(equivocation);

        let mut ancestors = round1;
        ancestors.sort_by_key(|r| (r.author.value() != 0, r.author));
        let block =
            VerifiedBlock::new_for_test(TestBlock::new(2, 0).set_ancestors_raw(ancestors).build());
        let minimal = inflater.serialize(&block, 0).unwrap();
        // The sender attaches an explicit digest for the equivocating slot, so its own
        // (ambiguous) view still inflates the exact referenced block.
        let (_signed, serialized) = inflater.inflate(&minimal).unwrap();
        assert_eq!(&serialized, block.serialized());
    }
}
