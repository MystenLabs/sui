// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transport-only "minimal block" codec for bandwidth-efficient block propagation.
//!
//! A [`MinimalBlock`] carries a block's BCS bytes with the ancestor vector stripped, plus
//! just enough structure — the ordered list of ancestor authors and per-ancestor overrides —
//! for the receiver to reconstruct the exact ancestor `BlockRef`s from its own DAG and
//! reserialize a byte-identical `SignedBlock`. Ancestor order must be preserved exactly:
//! the proposer appends score-sorted excluded ancestors after the authority-ordered ones,
//! so order is not derivable from authority indices.
//!
//! The sender's `claimed_block_digest` lets the receiver distinguish a failed local
//! reconstruction (digest mismatch => hand the block to the recovery manager, which
//! fetches the digest-verified original) from an invalid block (digest match + bad
//! signature => reject the peer).
//!
//! Minimal blocks are emitted only for live broadcasts on the validator `subscribe_blocks`
//! stream, and only for V1/V2 blocks — V3 is sent full until it ships and gets codec
//! coverage here.

use bytes::Bytes;
use consensus_config::{AuthorityIndex, Committee};
use consensus_types::block::{BlockDigest, BlockRef, Round};
use prost::Message as _;

use crate::block::{Block, BlockAPI as _, GENESIS_ROUND, SignedBlock, Slot, VerifiedBlock};

/// Ordered digest candidates for a slot. Backed in production by accepted DAG state
/// (authoritative, first) plus receipt-time hints (untrusted, after), and by simple maps
/// in tests.
pub(crate) trait AncestorDigestResolver {
    /// Ordered candidate digests at `slot`, including genesis blocks at round 0:
    /// accepted-DAG candidates first, then hints. May return 0 (unknown slot), 1
    /// (unique), or more (equivocation and/or hints). Callers bound how many
    /// candidates they will try; an untrusted hint must never displace or precede an
    /// accepted digest.
    fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest>;
}

/// Total reconstruction attempts per block. Bounds the work an equivocating or
/// hint-flooding peer can force; anything deeper falls back to fetching the full block.
const MAX_RECONSTRUCTION_ATTEMPTS: usize = 3;

/// Candidates tolerated at one slot before reconstruction gives up as ambiguous.
const MAX_SLOT_CANDIDATES: usize = 3;

/// The `Minimal` arm of the block wire envelope.
#[derive(Clone, prost::Message)]
struct MinimalBlock {
    /// bcs(Block) built with ancestors = [].
    #[prost(bytes = "bytes", tag = "1")]
    block_sans_ancestors: Bytes,
    /// Ancestor authors in exact proposal order (membership + order).
    #[prost(uint32, repeated, tag = "2")]
    ancestor_authors: Vec<u32>,
    /// Only the unusual ancestors: non-default round and/or explicit digest.
    #[prost(message, repeated, tag = "3")]
    overrides: Vec<AncestorOverride>,
    #[prost(bytes = "bytes", tag = "4")]
    signature: Bytes,
    /// 32 B digest of the full serialized SignedBlock: reconstruction proof + error distinction.
    #[prost(bytes = "bytes", tag = "5")]
    claimed_block_digest: Bytes,
}

#[derive(Clone, prost::Message)]
struct AncestorOverride {
    #[prost(uint32, tag = "1")]
    author: u32,
    /// Set only if the ancestor round is not `block.round - 1`.
    #[prost(uint32, optional, tag = "2")]
    round: Option<u32>,
    /// Set only if the slot equivocates, is below the recent-cache horizon, or is
    /// otherwise not safely resolvable by the receiver.
    #[prost(bytes = "bytes", optional, tag = "3")]
    digest: Option<Bytes>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MinimalBlockError {
    #[error("V3 blocks are not supported by the minimal-block codec")]
    UnsupportedVariant,
    #[error("failed to serialize block: {0}")]
    Serialization(#[from] bcs::Error),
}

/// Why a minimal block could not be inflated from local state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    /// No block known at the ancestor slot.
    MissingAncestor(Slot),
    /// Multiple blocks (equivocation) at the slot and no explicit digest hint.
    AmbiguousSlot(Slot),
    /// Reconstructed bytes do not hash to the claimed digest.
    DigestMismatch,
}

impl FallbackReason {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            FallbackReason::MissingAncestor(_) => "missing_ancestor",
            FallbackReason::AmbiguousSlot(_) => "ambiguous_slot",
            FallbackReason::DigestMismatch => "digest_mismatch",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InflateError {
    /// The encoding itself is invalid — a peer fault, not a local-state issue.
    #[error("malformed minimal block: {0}")]
    Malformed(String),
    /// Local state cannot resolve the block right now. The caller drops it; the
    /// missing-ancestor sync recovers it in the background once descendants reference it.
    #[error("cannot inflate block {block_ref}: {reason:?}")]
    NeedFullBlock {
        block_ref: BlockRef,
        reason: FallbackReason,
    },
}

/// Encodes a verified block into its minimal wire form.
///
/// Digests are omitted only for slots the receiver can resolve safely: unique in the
/// local DAG and either genesis or above `min_omittable_round` (the recent-cache
/// horizon). Everything else gets an explicit digest override.
pub(crate) fn serialize_minimal(
    block: &VerifiedBlock,
    resolver: &impl AncestorDigestResolver,
    min_omittable_round: Round,
) -> Result<Bytes, MinimalBlockError> {
    if matches!(**block, Block::V3(_)) {
        return Err(MinimalBlockError::UnsupportedVariant);
    }

    let default_round = block.round().saturating_sub(1);
    let mut ancestor_authors = Vec::with_capacity(block.ancestors().len());
    let mut overrides = vec![];
    for ancestor in block.ancestors() {
        ancestor_authors.push(ancestor.author.value() as u32);

        let round = (ancestor.round != default_round).then_some(ancestor.round);
        // The author's own parent always carries an explicit digest. It costs 32 bytes and
        // breaks the same-author cascade: a receiver that missed one of this author's blocks
        // could otherwise never inflate any later block from it (resolving the parent needs
        // the very block it lacks), so it would never reach acceptance, never register a
        // missing ancestor, and never fetch. With the parent digest present, inflation always
        // succeeds along an author's own chain and the block reaches `block_manager`, which
        // suspends it and lets the normal missing-ancestor sync recover the gap.
        let is_own_parent = ancestor.author == block.author();
        let can_omit_digest = !is_own_parent
            && (ancestor.round == GENESIS_ROUND || ancestor.round > min_omittable_round);
        let uniquely_resolvable = can_omit_digest
            && matches!(
                resolver.digests_at_slot(Slot::from(*ancestor)).as_slice(),
                [digest] if *digest == ancestor.digest
            );
        let digest = (!uniquely_resolvable).then(|| Bytes::copy_from_slice(&ancestor.digest.0));

        if round.is_some() || digest.is_some() {
            overrides.push(AncestorOverride {
                author: ancestor.author.value() as u32,
                round,
                digest,
            });
        }
    }

    let stripped = (**block).clone().with_ancestors(vec![]);
    let minimal = MinimalBlock {
        block_sans_ancestors: bcs::to_bytes(&stripped)?.into(),
        ancestor_authors,
        overrides,
        signature: block.signed_block().signature().clone(),
        claimed_block_digest: Bytes::copy_from_slice(&block.digest().0),
    };
    Ok(minimal.encode_to_vec().into())
}

/// Decodes and inflates a minimal block back into an unverified `SignedBlock` and its
/// canonical serialized bytes, ready for the normal `verify_and_vote` path.
///
/// The returned block is byte-identical to what the author signed iff the computed digest
/// matches `claimed_block_digest` (checked here). Signature verification is the caller's
/// responsibility — a signature failure after a digest match is a peer fault, not a
/// reconstruction failure.
pub(crate) fn deserialize_minimal(
    serialized: &[u8],
    committee: &Committee,
    expected_author: AuthorityIndex,
    resolver: &impl AncestorDigestResolver,
) -> Result<(SignedBlock, Bytes), InflateError> {
    let minimal = MinimalBlock::decode(serialized)
        .map_err(|e| InflateError::Malformed(format!("prost decode: {e}")))?;

    let claimed_digest: [u8; 32] = minimal
        .claimed_block_digest
        .as_ref()
        .try_into()
        .map_err(|_| InflateError::Malformed("claimed_block_digest must be 32 bytes".into()))?;
    let claimed_digest = BlockDigest(claimed_digest);

    let skeleton: Block = bcs::from_bytes(&minimal.block_sans_ancestors)
        .map_err(|e| InflateError::Malformed(format!("bcs decode: {e}")))?;
    if matches!(skeleton, Block::V3(_)) {
        return Err(InflateError::Malformed(
            "V3 blocks must be sent in full form".into(),
        ));
    }
    if !skeleton.ancestors().is_empty() {
        return Err(InflateError::Malformed(
            "block_sans_ancestors must have empty ancestors".into(),
        ));
    }
    // Cheap identity checks before any DAG access: the subscription stream carries only
    // the serving peer's own proposals, so a foreign author or epoch is a peer fault and
    // must not cost slot scans or a fallback fetch.
    if skeleton.author() != expected_author {
        return Err(InflateError::Malformed(format!(
            "block author {} does not match the sending peer {expected_author}",
            skeleton.author()
        )));
    }
    if skeleton.epoch() != committee.epoch() {
        return Err(InflateError::Malformed(format!(
            "block epoch {} does not match the committee epoch {}",
            skeleton.epoch(),
            committee.epoch()
        )));
    }

    // Bound and validate the untrusted ancestor structure before touching the DAG.
    let authors = &minimal.ancestor_authors;
    if authors.len() > committee.size() {
        return Err(InflateError::Malformed(format!(
            "{} ancestor authors exceeds committee size {}",
            authors.len(),
            committee.size()
        )));
    }
    if authors.is_empty() {
        // Every non-genesis block carries at least its own parent, and genesis blocks
        // are never broadcast; an empty ancestor list is a peer fault (and would
        // otherwise leave the reconstruction search with nothing to vary).
        return Err(InflateError::Malformed("empty ancestor authors".into()));
    }
    let mut seen = vec![false; committee.size()];
    for &author in authors {
        let Some(_) = committee.to_authority_index(author as usize) else {
            return Err(InflateError::Malformed(format!(
                "ancestor author {author} out of range"
            )));
        };
        if std::mem::replace(&mut seen[author as usize], true) {
            return Err(InflateError::Malformed(format!(
                "duplicate ancestor author {author}"
            )));
        }
    }
    if let Some(&first) = authors.first()
        && first as usize != skeleton.author().value()
    {
        return Err(InflateError::Malformed(
            "first ancestor must be the block author".into(),
        ));
    }
    let mut overridden = vec![false; committee.size()];
    for o in &minimal.overrides {
        if o.author as usize >= committee.size() || !seen[o.author as usize] {
            return Err(InflateError::Malformed(format!(
                "override for non-ancestor author {}",
                o.author
            )));
        }
        if std::mem::replace(&mut overridden[o.author as usize], true) {
            return Err(InflateError::Malformed(format!(
                "duplicate override for author {}",
                o.author
            )));
        }
        if let Some(round) = o.round
            && round >= skeleton.round()
        {
            return Err(InflateError::Malformed(format!(
                "override round {round} not below block round"
            )));
        }
        if let Some(digest) = &o.digest
            && digest.len() != 32
        {
            return Err(InflateError::Malformed(
                "override digest must be 32 bytes".into(),
            ));
        }
    }

    let block_ref = BlockRef::new(skeleton.round(), skeleton.author(), claimed_digest);
    let default_round = skeleton.round().saturating_sub(1);
    // Collect ordered digest candidates per ancestor. Slots the sender resolved explicitly
    // have exactly one candidate; omitted slots take the resolver's ordered candidates
    // (accepted-DAG digests first, then receipt-time hints — see AncestorDigestResolver).
    let mut candidates: Vec<(Round, AuthorityIndex, Vec<BlockDigest>)> =
        Vec::with_capacity(authors.len());
    for &author in authors {
        let authority = committee
            .to_authority_index(author as usize)
            .expect("validated above");
        let o = minimal.overrides.iter().find(|o| o.author == author);
        let round = o.and_then(|o| o.round).unwrap_or(default_round);
        let slot = Slot::new(round, authority);
        let slot_candidates = match o.and_then(|o| o.digest.as_ref()) {
            Some(digest) => {
                vec![BlockDigest(
                    digest.as_ref().try_into().expect("validated above"),
                )]
            }
            None => {
                let resolved = resolver.digests_at_slot(slot);
                if resolved.is_empty() {
                    return Err(InflateError::NeedFullBlock {
                        block_ref,
                        reason: FallbackReason::MissingAncestor(slot),
                    });
                }
                if resolved.len() > MAX_SLOT_CANDIDATES {
                    // Heavy equivocation (or hint flooding): reconstruction search space
                    // is not worth exploring; the full block resolves it.
                    return Err(InflateError::NeedFullBlock {
                        block_ref,
                        reason: FallbackReason::AmbiguousSlot(slot),
                    });
                }
                resolved
            }
        };
        candidates.push((round, authority, slot_candidates));
    }

    // Bounded reconstruction search: the baseline attempt takes every slot's first
    // candidate; each further attempt varies a single ambiguous slot to its next
    // candidate. The budget bounds TOTAL work per block, not per slot, so several
    // ambiguous slots cannot multiply into a combinatorial search. The claimed digest
    // selects the correct combination; equivocation deep enough to defeat this bound is
    // resolved by fetching the full block instead.
    let mut attempts_left = MAX_RECONSTRUCTION_ATTEMPTS;
    let mut variant: Option<(usize, usize)> = None; // (slot index, candidate index)
    loop {
        let mut ancestors = Vec::with_capacity(candidates.len());
        for (index, (round, authority, slot_candidates)) in candidates.iter().enumerate() {
            let candidate = match variant {
                Some((slot, choice)) if slot == index => slot_candidates[choice],
                _ => slot_candidates[0],
            };
            ancestors.push(BlockRef::new(*round, *authority, candidate));
        }
        let signed_block = SignedBlock::from_parts(
            skeleton.clone().with_ancestors(ancestors),
            minimal.signature.clone(),
        );
        let serialized_block = signed_block
            .serialize()
            .map_err(|e| InflateError::Malformed(format!("bcs encode: {e}")))?;
        if VerifiedBlock::compute_digest(&serialized_block) == claimed_digest {
            return Ok((signed_block, serialized_block));
        }
        attempts_left -= 1;
        if attempts_left == 0 {
            return Err(InflateError::NeedFullBlock {
                block_ref,
                reason: FallbackReason::DigestMismatch,
            });
        }
        variant = match next_variant(&candidates, variant) {
            Some(v) => Some(v),
            None => {
                return Err(InflateError::NeedFullBlock {
                    block_ref,
                    reason: FallbackReason::DigestMismatch,
                });
            }
        };
    }
}

/// Advances the single-slot variation cursor over ambiguous slots, in slot order then
/// candidate order. Returns None when all single-slot variations are exhausted.
fn next_variant(
    candidates: &[(Round, AuthorityIndex, Vec<BlockDigest>)],
    current: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    if candidates.is_empty() {
        return None;
    }
    let (mut slot, mut choice) = match current {
        None => (0, 0),
        Some((slot, choice)) => (slot, choice),
    };
    loop {
        choice += 1;
        if choice < candidates[slot].2.len() {
            return Some((slot, choice));
        }
        slot += 1;
        choice = 0;
        if slot >= candidates.len() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use consensus_config::AuthorityIndex;
    use rand::{RngCore as _, SeedableRng as _, rngs::StdRng};

    use super::*;
    use crate::{
        block::{BlockV1, TestBlock, Transaction, genesis_blocks},
        context::Context,
    };

    #[derive(Default)]
    struct MapResolver(BTreeMap<Slot, Vec<BlockDigest>>);

    impl MapResolver {
        fn insert(&mut self, block_ref: BlockRef) {
            self.0
                .entry(Slot::from(block_ref))
                .or_default()
                .push(block_ref.digest);
        }
    }

    impl AncestorDigestResolver for MapResolver {
        fn digests_at_slot(&self, slot: Slot) -> Vec<BlockDigest> {
            self.0.get(&slot).cloned().unwrap_or_default()
        }
    }

    fn test_digest(rng: &mut StdRng) -> BlockDigest {
        let mut digest = [0u8; 32];
        rng.fill_bytes(&mut digest);
        BlockDigest(digest)
    }

    /// Refs for one ancestor per authority at `round`, own author (0) first, with
    /// random digests registered in the resolver.
    fn ancestor_refs(
        committee_size: usize,
        round: Round,
        resolver: &mut MapResolver,
        rng: &mut StdRng,
    ) -> Vec<BlockRef> {
        (0..committee_size)
            .map(|authority| {
                let block_ref = BlockRef::new(
                    round,
                    AuthorityIndex::new_for_test(authority as u32),
                    test_digest(rng),
                );
                resolver.insert(block_ref);
                block_ref
            })
            .collect()
    }

    fn sign(
        block: Block,
        context: &Context,
        key_pairs: &[(
            consensus_config::NetworkKeyPair,
            consensus_config::ProtocolKeyPair,
        )],
    ) -> VerifiedBlock {
        let author = block.author().value();
        let signed = SignedBlock::new(block, &key_pairs[author].1).unwrap();
        let serialized = signed.serialize().unwrap();
        let verified = VerifiedBlock::new_verified(signed, serialized);
        verified.signed_block().verify_signature(context).unwrap();
        verified
    }

    fn roundtrip(
        block: &VerifiedBlock,
        context: &Context,
        resolver: &impl AncestorDigestResolver,
        min_omittable_round: Round,
    ) -> Bytes {
        let minimal = serialize_minimal(block, resolver, min_omittable_round).unwrap();
        let (signed, serialized) =
            deserialize_minimal(&minimal, &context.committee, block.author(), resolver).unwrap();
        assert_eq!(&serialized, block.serialized());
        assert_eq!(VerifiedBlock::compute_digest(&serialized), block.digest());
        signed.verify_signature(context).unwrap();
        minimal
    }

    #[tokio::test]
    async fn roundtrip_v2_common_case() {
        let (context, key_pairs) = Context::new_for_test(7);
        let mut rng = StdRng::seed_from_u64(7);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(7, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors)
            .set_transactions(vec![Transaction::new(vec![1, 2, 3])])
            .build();
        let block = sign(block, &context, &key_pairs);

        let minimal = roundtrip(&block, &context, &resolver, 0);
        // Common case: the only override is the author's own parent, whose digest always
        // rides explicitly (cascade-breaking); every other digest resolves locally.
        let decoded = MinimalBlock::decode(minimal.as_ref()).unwrap();
        assert_eq!(decoded.overrides.len(), 1);
        assert_eq!(decoded.overrides[0].author, 0);
        assert!(decoded.overrides[0].digest.is_some());
        assert!(minimal.len() < block.serialized().len());
    }

    #[tokio::test]
    async fn roundtrip_v1() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(4);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 5, &mut resolver, &mut rng);
        let block = Block::V1(BlockV1::new(
            0,
            6,
            AuthorityIndex::new_for_test(0),
            1000,
            ancestors,
            vec![Transaction::new(vec![9; 100])],
            vec![],
            vec![],
        ));
        let block = sign(block, &context, &key_pairs);
        roundtrip(&block, &context, &resolver, 0);
    }

    #[tokio::test]
    async fn roundtrip_preserves_non_authority_order() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(42);
        let mut resolver = MapResolver::default();
        let mut ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        // Own-first is required, but the tail order is proposer-defined (score-sorted).
        ancestors[1..].reverse();
        let block = TestBlock::new(10, 0).set_ancestors_raw(ancestors).build();
        let block = sign(block, &context, &key_pairs);
        roundtrip(&block, &context, &resolver, 0);
    }

    #[tokio::test]
    async fn roundtrip_with_overrides() {
        let (context, key_pairs) = Context::new_for_test(6);
        let mut rng = StdRng::seed_from_u64(6);
        let mut resolver = MapResolver::default();
        let mut ancestors = ancestor_refs(6, 19, &mut resolver, &mut rng);
        // Ancestor 1: older round => round override.
        ancestors[1] = BlockRef::new(15, AuthorityIndex::new_for_test(1), test_digest(&mut rng));
        resolver.insert(ancestors[1]);
        // Ancestor 2: equivocating slot => sender must attach an explicit digest.
        resolver.insert(BlockRef::new(
            19,
            AuthorityIndex::new_for_test(2),
            test_digest(&mut rng),
        ));
        let block = TestBlock::new(20, 0).set_ancestors_raw(ancestors).build();
        let block = sign(block, &context, &key_pairs);

        // With min_omittable_round = 16, the round-15 ancestor also needs an explicit
        // digest (below the recent-cache horizon), on top of its round override. The third
        // override is the own parent's always-explicit digest.
        let minimal = roundtrip(&block, &context, &resolver, 16);
        let decoded = MinimalBlock::decode(minimal.as_ref()).unwrap();
        assert_eq!(decoded.overrides.len(), 3);
        assert!(decoded.overrides.iter().all(|o| o.digest.is_some()));
    }

    /// A receiver that dropped block X from an author must still inflate the author's next
    /// block X': the own-parent digest rides explicitly, so inflation never needs to resolve
    /// the one slot the receiver is missing, and the reconstructed ancestors carry X's exact
    /// ref for block_manager to suspend on and sync to fetch. Without this, dropping one
    /// block would make every later block from that author un-inflatable.
    #[tokio::test]
    async fn own_parent_digest_breaks_missing_parent_cascade() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(29);
        let mut sender = MapResolver::default();
        let round9 = ancestor_refs(4, 9, &mut sender, &mut rng);

        // X: author 0's round-10 block, which the receiver dropped.
        let x = TestBlock::new(10, 0)
            .set_ancestors_raw(round9.clone())
            .build();
        let x = sign(x, &context, &key_pairs);
        sender.insert(x.reference());

        // The receiver holds everything at round 10 EXCEPT X.
        let mut receiver = MapResolver::default();
        let mut round10 = vec![x.reference()];
        for author in 1..4u32 {
            let block_ref = BlockRef::new(
                10,
                AuthorityIndex::new_for_test(author),
                test_digest(&mut rng),
            );
            sender.insert(block_ref);
            receiver.insert(block_ref);
            round10.push(block_ref);
        }

        // X': author 0's next block, whose own parent is the dropped X.
        let x2 = TestBlock::new(11, 0).set_ancestors_raw(round10).build();
        let x2 = sign(x2, &context, &key_pairs);
        let minimal = serialize_minimal(&x2, &sender, 0).unwrap();

        let (_signed, serialized) =
            deserialize_minimal(&minimal, &context.committee, x2.author(), &receiver).unwrap();
        // Byte-identity proves the reconstructed ancestors include X's exact ref.
        assert_eq!(&serialized, x2.serialized());
        assert_eq!(x2.ancestors()[0], x.reference());
    }

    #[tokio::test]
    async fn roundtrip_genesis_ancestors() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut resolver = MapResolver::default();
        let genesis: Vec<_> = genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        for genesis_ref in &genesis {
            resolver.insert(*genesis_ref);
        }
        let block = TestBlock::new(1, 0).set_ancestors_raw(genesis).build();
        let block = sign(block, &context, &key_pairs);
        // Genesis digests are omittable even when the horizon says "explicit below round 5";
        // only the own parent (itself genesis here) carries its always-explicit digest.
        let minimal = roundtrip(&block, &context, &resolver, 5);
        let decoded = MinimalBlock::decode(minimal.as_ref()).unwrap();
        assert_eq!(decoded.overrides.len(), 1);
        assert_eq!(decoded.overrides[0].author, 0);
        assert!(decoded.overrides[0].digest.is_some());
    }

    #[tokio::test]
    async fn missing_and_ambiguous_slots_fall_back() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(11);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();

        // Receiver missing a slot entirely => MissingAncestor with the fetchable ref.
        let mut missing = MapResolver::default();
        for ancestor in &ancestors[..3] {
            missing.insert(*ancestor);
        }
        match deserialize_minimal(&minimal, &context.committee, block.author(), &missing)
            .map(|_| ())
        {
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                assert_eq!(block_ref, block.reference());
                assert_eq!(
                    reason,
                    FallbackReason::MissingAncestor(Slot::from(ancestors[3]))
                );
            }
            other => panic!("expected MissingAncestor fallback, got {other:?}"),
        }

        // Receiver seeing an equivocation the sender didn't: bounded multi-try lets the
        // claimed digest select the right candidate — reconstruction now SUCCEEDS.
        let mut ambiguous = MapResolver::default();
        for ancestor in &ancestors {
            ambiguous.insert(*ancestor);
        }
        ambiguous.insert(BlockRef::new(9, ancestors[3].author, test_digest(&mut rng)));
        let (_signed, serialized) =
            deserialize_minimal(&minimal, &context.committee, block.author(), &ambiguous).unwrap();
        assert_eq!(&serialized, block.serialized());

        // Beyond the per-slot candidate bound, the search space is refused outright.
        let mut flooded = MapResolver::default();
        for ancestor in &ancestors {
            flooded.insert(*ancestor);
        }
        for _ in 0..MAX_SLOT_CANDIDATES {
            flooded.insert(BlockRef::new(9, ancestors[3].author, test_digest(&mut rng)));
        }
        match deserialize_minimal(&minimal, &context.committee, block.author(), &flooded)
            .map(|_| ())
        {
            Err(InflateError::NeedFullBlock { reason, .. }) => {
                assert_eq!(
                    reason,
                    FallbackReason::AmbiguousSlot(Slot::from(ancestors[3]))
                );
            }
            other => panic!("expected AmbiguousSlot fallback, got {other:?}"),
        }
    }

    /// The reconstruction budget bounds TOTAL attempts, not per-slot candidates: with
    /// two ambiguous slots whose correct digests both sit in second position, no
    /// single-slot variation within the budget can satisfy the claimed digest, so the
    /// codec refuses (DigestMismatch => fetch) instead of searching combinatorially.
    #[tokio::test]
    async fn reconstruction_attempt_budget_is_global() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(31);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();

        // Two slots each gain a bogus candidate that sorts BEFORE the real digest.
        let mut skewed = MapResolver::default();
        for (index, ancestor) in ancestors.iter().enumerate() {
            if index == 2 || index == 3 {
                skewed.insert(BlockRef::new(9, ancestor.author, BlockDigest::MIN));
            }
            skewed.insert(*ancestor);
        }
        match deserialize_minimal(&minimal, &context.committee, block.author(), &skewed).map(|_| ())
        {
            // Baseline (both wrong) + two single-slot variations cannot fix two slots
            // at once: the budget correctly refuses rather than expanding the search.
            Err(InflateError::NeedFullBlock { reason, .. }) => {
                assert_eq!(reason, FallbackReason::DigestMismatch);
            }
            other => panic!("expected DigestMismatch fallback, got {other:?}"),
        }

        // One ambiguous slot alone resolves within the budget.
        let mut single = MapResolver::default();
        for (index, ancestor) in ancestors.iter().enumerate() {
            if index == 2 {
                single.insert(BlockRef::new(9, ancestor.author, BlockDigest::MIN));
            }
            single.insert(*ancestor);
        }
        let (_signed, serialized) =
            deserialize_minimal(&minimal, &context.committee, block.author(), &single).unwrap();
        assert_eq!(&serialized, block.serialized());
    }

    #[tokio::test]
    async fn wrong_single_block_at_slot_is_digest_mismatch() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(13);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();

        // Receiver holds a *different* single block at one slot (asymmetric equivocation):
        // reconstruction succeeds but produces the wrong bytes => DigestMismatch.
        let mut skewed = MapResolver::default();
        for ancestor in &ancestors[..3] {
            skewed.insert(*ancestor);
        }
        skewed.insert(BlockRef::new(9, ancestors[3].author, test_digest(&mut rng)));
        match deserialize_minimal(&minimal, &context.committee, block.author(), &skewed).map(|_| ())
        {
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                assert_eq!(block_ref, block.reference());
                assert_eq!(reason, FallbackReason::DigestMismatch);
            }
            other => panic!("expected DigestMismatch fallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_inputs_are_rejected_without_panic() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(17);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0).set_ancestors_raw(ancestors).build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();
        let committee = &context.committee;

        // Garbage bytes.
        assert!(matches!(
            deserialize_minimal(b"garbage", committee, block.author(), &resolver),
            Err(InflateError::Malformed(_))
        ));

        let decoded = MinimalBlock::decode(minimal.as_ref()).unwrap();
        let tamper = |f: &dyn Fn(&mut MinimalBlock)| {
            let mut m = decoded.clone();
            f(&mut m);
            deserialize_minimal(&m.encode_to_vec(), committee, block.author(), &resolver)
        };

        // Tampered claimed digest => mismatch fallback, not acceptance.
        assert!(matches!(
            tamper(&|m| {
                let mut digest = m.claimed_block_digest.to_vec();
                digest[0] ^= 1;
                m.claimed_block_digest = digest.into();
            }),
            Err(InflateError::NeedFullBlock {
                reason: FallbackReason::DigestMismatch,
                ..
            })
        ));
        // Bad digest length.
        assert!(matches!(
            tamper(&|m| m.claimed_block_digest = Bytes::from_static(&[1, 2, 3])),
            Err(InflateError::Malformed(_))
        ));
        // Empty ancestor list (would otherwise reach the reconstruction search with
        // nothing to vary — regression guard for a remote panic).
        assert!(matches!(
            tamper(&|m| {
                m.ancestor_authors.clear();
                m.overrides.clear();
            }),
            Err(InflateError::Malformed(_))
        ));
        // Duplicate ancestor author.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors[2] = m.ancestor_authors[1]),
            Err(InflateError::Malformed(_))
        ));
        // Author out of committee range.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors[2] = 100),
            Err(InflateError::Malformed(_))
        ));
        // More authors than the committee.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors = (0..10).collect()),
            Err(InflateError::Malformed(_))
        ));
        // First ancestor not the block author.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors.swap(0, 1)),
            Err(InflateError::Malformed(_))
        ));
        // Override for a non-ancestor author.
        assert!(matches!(
            tamper(&|m| m.overrides.push(AncestorOverride {
                author: 50,
                round: None,
                digest: None
            })),
            Err(InflateError::Malformed(_))
        ));
        // Override round not below the block round.
        assert!(matches!(
            tamper(&|m| m.overrides.push(AncestorOverride {
                author: 1,
                round: Some(10),
                digest: None
            })),
            Err(InflateError::Malformed(_))
        ));
        // Skeleton with non-empty ancestors (would double-count on splice).
        assert!(matches!(
            tamper(&|m| {
                let skeleton: Block = bcs::from_bytes(&m.block_sans_ancestors).unwrap();
                let skeleton = skeleton.with_ancestors(vec![BlockRef::MIN]);
                m.block_sans_ancestors = bcs::to_bytes(&skeleton).unwrap().into();
            }),
            Err(InflateError::Malformed(_))
        ));
    }

    #[tokio::test]
    async fn v3_is_rejected_by_encoder() {
        let v3 = Block::V3(crate::block::BlockV3::new(
            0,
            10,
            AuthorityIndex::new_for_test(0),
            1000,
            vec![],
            vec![],
            vec![],
            5,
            vec![],
            vec![],
        ));
        let block = VerifiedBlock::new_for_test(v3);
        assert!(matches!(
            serialize_minimal(&block, &MapResolver::default(), 0),
            Err(MinimalBlockError::UnsupportedVariant)
        ));
    }

    /// Post-zstd wire bytes of full vs minimal encoding for a mainnet-shaped block
    /// (~100 ancestors, high-entropy digests and payload): the bandwidth saving is the
    /// feature's purpose, so a codec change that erodes it below 2x must fail loudly.
    #[tokio::test]
    async fn wire_size_savings_mainnet_shape() {
        const COMMITTEE_SIZE: usize = 100;
        let (context, key_pairs) = Context::new_for_test(COMMITTEE_SIZE);
        let mut rng = StdRng::seed_from_u64(2026);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(COMMITTEE_SIZE, 999, &mut resolver, &mut rng);
        // ~1 KB of high-entropy transaction payload, like real (already-compressed) txns.
        let transactions = (0..5)
            .map(|_| {
                let mut data = vec![0u8; 200];
                rng.fill_bytes(&mut data);
                Transaction::new(data)
            })
            .collect();
        let block = TestBlock::new(1000, 0)
            .set_ancestors_raw(ancestors)
            .set_transactions(transactions)
            .build();
        let block = sign(block, &context, &key_pairs);

        let minimal = roundtrip(&block, &context, &resolver, 0);
        let full = block.serialized();

        let zstd_len = |bytes: &[u8]| zstd::encode_all(bytes, 3).unwrap().len();
        let (full_raw, minimal_raw) = (full.len(), minimal.len());
        let (full_zstd, minimal_zstd) = (zstd_len(full), zstd_len(&minimal));
        eprintln!(
            "wire size, mainnet shape ({COMMITTEE_SIZE} ancestors): \
             full {full_raw} B raw / {full_zstd} B zstd; \
             minimal {minimal_raw} B raw / {minimal_zstd} B zstd; \
             post-zstd saving {:.1}%",
            100.0 * (1.0 - minimal_zstd as f64 / full_zstd as f64)
        );
        // The ancestor vector dominates the full block; minimal must reclaim most of it
        // even after zstd has had its shot at the full encoding.
        assert!(minimal_zstd * 2 < full_zstd);
    }

    #[tokio::test]
    async fn author_and_epoch_mismatches_rejected_before_dag_access() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(23);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();

        // The subscription stream only carries the sending peer's own proposals: a block
        // claiming a different author is a peer fault, rejected without slot lookups.
        assert!(matches!(
            deserialize_minimal(
                &minimal,
                &context.committee,
                AuthorityIndex::new_for_test(1),
                &resolver,
            )
            .map(|_| ()),
            Err(InflateError::Malformed(_))
        ));

        // Same for a block from another epoch.
        let stale = TestBlock::new(10, 0)
            .set_epoch(7)
            .set_ancestors_raw(ancestors)
            .build();
        let stale = sign(stale, &context, &key_pairs);
        let stale_minimal = serialize_minimal(&stale, &resolver, 0).unwrap();
        assert!(matches!(
            deserialize_minimal(
                &stale_minimal,
                &context.committee,
                stale.author(),
                &resolver,
            )
            .map(|_| ()),
            Err(InflateError::Malformed(_))
        ));
    }

    /// Randomized codec-identity sweep: diverse blocks (ancestor counts, shuffled tail
    /// order, off-parent rounds, equivocating slots, cache horizons, payload shapes)
    /// must reconstruct byte-identically. Targets the ordering/override bug class.
    #[tokio::test]
    async fn randomized_codec_identity_sweep() {
        const COMMITTEE_SIZE: usize = 10;
        const ITERATIONS: u64 = 200;
        let (context, key_pairs) = Context::new_for_test(COMMITTEE_SIZE);

        for seed in 0..ITERATIONS {
            let mut rng = StdRng::seed_from_u64(seed);
            let round = 2 + (rng.next_u32() % 500);
            let mut resolver = MapResolver::default();

            // Own ancestor first, then a shuffled subset of other authorities — some at
            // parent round, some older (round overrides), some in equivocating slots.
            let mut others: Vec<u32> = (1..COMMITTEE_SIZE as u32).collect();
            for i in (1..others.len()).rev() {
                others.swap(i, (rng.next_u32() as usize) % (i + 1));
            }
            let ancestor_count = 1 + (rng.next_u32() as usize) % others.len();
            let mut ancestors = vec![BlockRef::new(
                round - 1,
                AuthorityIndex::new_for_test(0),
                test_digest(&mut rng),
            )];
            for &author in &others[..ancestor_count] {
                let ancestor_round = if rng.next_u32() % 4 == 0 {
                    1 + rng.next_u32() % (round - 1)
                } else {
                    round - 1
                };
                ancestors.push(BlockRef::new(
                    ancestor_round,
                    AuthorityIndex::new_for_test(author),
                    test_digest(&mut rng),
                ));
            }
            for ancestor in &ancestors {
                resolver.insert(*ancestor);
                // Occasionally equivocate the slot so the sender must attach a digest.
                if rng.next_u32() % 5 == 0 {
                    resolver.insert(BlockRef::new(
                        ancestor.round,
                        ancestor.author,
                        test_digest(&mut rng),
                    ));
                }
            }

            let transactions = (0..(rng.next_u32() % 4))
                .map(|_| {
                    let mut data = vec![0u8; 1 + (rng.next_u32() as usize % 300)];
                    rng.fill_bytes(&mut data);
                    Transaction::new(data)
                })
                .collect();
            let block = TestBlock::new(round, 0)
                .set_ancestors_raw(ancestors)
                .set_transactions(transactions)
                .set_timestamp_ms(rng.next_u32() as u64)
                .build();
            let block = sign(block, &context, &key_pairs);

            let min_omittable_round = rng.next_u32() % round;
            let minimal = serialize_minimal(&block, &resolver, min_omittable_round).unwrap();
            let (_signed, serialized) =
                deserialize_minimal(&minimal, &context.committee, block.author(), &resolver)
                    .unwrap_or_else(|e| panic!("seed {seed} failed to inflate: {e}"));
            assert_eq!(&serialized, block.serialized(), "seed {seed}");
        }
    }

    /// Decoder fuzz: random mutations and truncations of a valid encoding must produce
    /// a clean error or a digest-mismatch fallback — never a panic, never acceptance.
    #[tokio::test]
    async fn decoder_mutation_fuzz_never_panics() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(31);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors)
            .set_transactions(vec![Transaction::new(vec![7; 64])])
            .build();
        let block = sign(block, &context, &key_pairs);
        let minimal = serialize_minimal(&block, &resolver, 0).unwrap();

        for _ in 0..500 {
            let mut bytes = minimal.to_vec();
            match rng.next_u32() % 3 {
                // Flip 1-4 bytes anywhere.
                0 => {
                    for _ in 0..1 + rng.next_u32() % 4 {
                        let i = rng.next_u32() as usize % bytes.len();
                        bytes[i] ^= (1 + rng.next_u32() % 255) as u8;
                    }
                }
                // Truncate at a random point.
                1 => bytes.truncate(rng.next_u32() as usize % bytes.len()),
                // Pure garbage of random length.
                _ => {
                    bytes = vec![0u8; rng.next_u32() as usize % 512];
                    rng.fill_bytes(&mut bytes);
                }
            }
            // A mutation that still inflates must have produced the original bytes
            // (e.g. it only touched redundant varint encoding); anything else must
            // have failed the digest check on the way out.
            if let Ok((_signed, serialized)) =
                deserialize_minimal(&bytes, &context.committee, block.author(), &resolver)
            {
                assert_eq!(&serialized, block.serialized());
            }
        }
    }
}
