// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transport-only "slim block" codec for bandwidth-efficient block propagation.
//!
//! A [`SlimBlock`] carries a block's BCS bytes with the ancestor vector stripped, plus
//! just enough structure — the ordered list of ancestor authors and per-ancestor overrides —
//! for the receiver to rebuild the exact ancestor `BlockRef`s from its own DAG and
//! reserialize a byte-identical `SignedBlock`. The author's signature covers those
//! bytes, so the ancestor sequence has to be restored exactly as proposed. It cannot
//! be derived: the proposer starts authority-ordered, then appends any ancestor it
//! promotes back in to reach the parent-round quorum, and those go in score order.
//! Hence an ordered list of authors on the wire rather than a set.
//!
//! The sender's `claimed_block_digest` separates a failed rebuild (digest mismatch,
//! a local-state problem) from an invalid block (digest match but bad signature, a
//! peer fault).
//!
//! Slim blocks are emitted only for live broadcasts on the validator `subscribe_blocks`
//! stream, for every block version — `Block::with_ancestors` rebuilds V1/V2/V3 alike.
//!
//! The free functions here take an [`AncestorDigestResolver`] and so cannot reach node
//! state; [`SlimBlockCodec`] is the binding that supplies one from `DagState`.

use std::sync::Arc;

use bytes::Bytes;
use consensus_config::{AuthorityIndex, Committee, DIGEST_LENGTH};
use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::RwLock;
use prost::Message as _;

use crate::{
    block::{
        Block, BlockAPI as _, GENESIS_ROUND, SignedBlock, Slot, SlotDigest, VerifiedBlock,
        genesis_blocks,
    },
    context::Context,
    dag_state::DagState,
};

/// Resolves what local state holds at a slot, so encoding can decide which digests are
/// safe to omit and decoding can restore them.
pub(crate) trait AncestorDigestResolver {
    /// What `slot` resolves to, counting genesis blocks at round 0. Only the three
    /// outcomes are needed, never the full candidate list: an omitted digest is
    /// recoverable exactly when the slot is unique.
    fn digest_at_slot(&self, slot: Slot) -> SlotDigest;
}

/// The `Slim` arm of the block wire envelope.
#[derive(Clone, prost::Message)]
struct SlimBlock {
    /// bcs(Block) built with ancestors = [].
    #[prost(bytes = "bytes", tag = "1")]
    block: Bytes,
    /// Ancestor authors in exact proposal order (membership + order).
    #[prost(uint32, repeated, tag = "2")]
    ancestor_authors: Vec<u32>,
    /// Only the unusual ancestors: non-default round and/or explicit digest.
    #[prost(message, repeated, tag = "3")]
    overrides: Vec<AncestorOverride>,
    #[prost(bytes = "bytes", tag = "4")]
    signature: Bytes,
    /// Digest of the full serialized SignedBlock: rebuild proof + error distinction.
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

/// Why a slim block could not be decoded from local state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    /// The complete frontier of ancestor slots with no accepted block, in canonical
    /// ancestor order. Collected in ONE pass so a recovery wait can register on all
    /// of them and wake once, instead of rediscovering them one wake at a time.
    MissingAncestors(Vec<Slot>),
    /// More equivocating candidates at the slot than a rebuild will search.
    AmbiguousSlot(Slot),
    /// Rebuilt bytes do not hash to the claimed digest.
    DigestMismatch,
}

impl FallbackReason {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            FallbackReason::MissingAncestors(_) => "missing_ancestor",
            FallbackReason::AmbiguousSlot(_) => "ambiguous_slot",
            FallbackReason::DigestMismatch => "digest_mismatch",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DecodeError {
    /// The encoding itself is invalid — a peer fault, not a local-state issue.
    #[error("malformed slim block: {0}")]
    Malformed(String),
    /// Local state cannot resolve the block right now.
    #[error("cannot decode block {block_ref}: {reason:?}")]
    NeedFullBlock {
        block_ref: BlockRef,
        reason: FallbackReason,
    },
}

impl DecodeError {
    /// Stable label for metrics; distinguishes a peer fault from local state
    /// simply not being ready yet.
    pub(crate) fn metric_label(&self) -> &'static str {
        match self {
            DecodeError::Malformed(_) => "malformed",
            DecodeError::NeedFullBlock { reason, .. } => reason.label(),
        }
    }
}

/// The explicit digest to put on the wire for `ancestor`, or `None` when the receiver is
/// guaranteed to resolve the slot to this exact digest on its own.
fn digest_override(
    block: &VerifiedBlock,
    ancestor: &BlockRef,
    resolver: &impl AncestorDigestResolver,
    min_omittable_round: Round,
) -> Option<Bytes> {
    let explicit = || Some(Bytes::copy_from_slice(&ancestor.digest.0));

    // The author's own parent always carries an explicit digest. It is the only
    // ancestor edge that chains within one author: a missing parent leaves every later
    // block from that author undecodable too, since each needs the one before it.
    // Explicit here, the block still decodes and reaches `block_manager`, whose
    // suspension drives the normal missing-ancestor sync.
    if ancestor.author == block.author() {
        return explicit();
    }
    // Below the horizon the receiver may have collected the slot already; genesis is
    // exempt because it resolves from the committee rather than the DAG.
    if ancestor.round != GENESIS_ROUND && ancestor.round <= min_omittable_round {
        return explicit();
    }
    // Safe to omit only where local state resolves the slot uniquely to this digest.
    match resolver.digest_at_slot(Slot::from(*ancestor)) {
        SlotDigest::Unique(digest) if digest == ancestor.digest => None,
        _ => explicit(),
    }
}

/// The override list for `block`, the only part of encoding that reads local state.
fn ancestor_overrides(
    block: &VerifiedBlock,
    resolver: &impl AncestorDigestResolver,
    min_omittable_round: Round,
) -> Vec<AncestorOverride> {
    let parent_round = block.round().saturating_sub(1);
    let mut overrides = vec![];
    for ancestor in block.ancestors() {
        let round = (ancestor.round != parent_round).then_some(ancestor.round);
        let digest = digest_override(block, ancestor, resolver, min_omittable_round);

        if round.is_some() || digest.is_some() {
            overrides.push(AncestorOverride {
                author: ancestor.author.value() as u32,
                round,
                digest,
            });
        }
    }
    overrides
}

/// Encodes a verified block into its slim wire form: the block with an empty ancestor
/// vector, the ancestor authors in proposal order, and `overrides` for the slots the
/// receiver cannot infer.
fn serialize_slim(
    block: &VerifiedBlock,
    overrides: Vec<AncestorOverride>,
) -> Result<Bytes, bcs::Error> {
    let ancestor_authors = block
        .ancestors()
        .iter()
        .map(|ancestor| ancestor.author.value() as u32)
        .collect();

    let stripped = (**block).clone().with_ancestors(vec![]);
    let slim = SlimBlock {
        block: bcs::to_bytes(&stripped)?.into(),
        ancestor_authors,
        overrides,
        signature: block.signed_block().signature().clone(),
        claimed_block_digest: Bytes::copy_from_slice(&block.digest().0),
    };
    Ok(slim.encode_to_vec().into())
}

/// A structurally validated envelope: everything checkable without reading local state.
/// Fields stay private so no caller can assemble a `SignedBlock` that skipped the digest
/// gate in [`rebuild_from_slim`].
pub(crate) struct ParsedEnvelope {
    /// The author's block with an empty ancestor vector, decoded once here so the
    /// resolve and rebuild phases share the work.
    stripped_block: Block,
    ancestor_authors: Vec<u32>,
    overrides: Vec<AncestorOverride>,
    signature: Bytes,
    block_ref: BlockRef,
    claimed_digest: BlockDigest,
}

/// Structural phase: decode and every validation that needs no DAG access.
/// `Malformed` is the only error this phase can produce.
pub(crate) fn parse_slim(
    serialized: &[u8],
    committee: &Committee,
    expected_author: AuthorityIndex,
) -> Result<ParsedEnvelope, DecodeError> {
    let slim = <SlimBlock as prost::Message>::decode(serialized)
        .map_err(|e| DecodeError::Malformed(format!("prost decode: {e}")))?;

    let claimed_digest: [u8; DIGEST_LENGTH] =
        slim.claimed_block_digest.as_ref().try_into().map_err(|_| {
            DecodeError::Malformed(format!(
                "claimed_block_digest must be {DIGEST_LENGTH} bytes"
            ))
        })?;
    let claimed_digest = BlockDigest(claimed_digest);

    let stripped_block: Block = bcs::from_bytes(&slim.block)
        .map_err(|e| DecodeError::Malformed(format!("bcs decode: {e}")))?;
    if !stripped_block.ancestors().is_empty() {
        return Err(DecodeError::Malformed(
            "block must have empty ancestors".into(),
        ));
    }
    // Cheap identity checks before any DAG access: the subscription stream carries only
    // the serving peer's own proposals, so a foreign author or epoch is a peer fault and
    // must not cost slot scans or a fallback fetch.
    if stripped_block.author() != expected_author {
        return Err(DecodeError::Malformed(format!(
            "block author {} does not match the sending peer {expected_author}",
            stripped_block.author()
        )));
    }
    if stripped_block.epoch() != committee.epoch() {
        return Err(DecodeError::Malformed(format!(
            "block epoch {} does not match the committee epoch {}",
            stripped_block.epoch(),
            committee.epoch()
        )));
    }

    // Bound and validate the untrusted ancestor structure before touching the DAG.
    let authors = &slim.ancestor_authors;
    if authors.len() > committee.size() {
        return Err(DecodeError::Malformed(format!(
            "{} ancestor authors exceeds committee size {}",
            authors.len(),
            committee.size()
        )));
    }
    if authors.is_empty() {
        // Every non-genesis block carries at least its own parent, and genesis blocks
        // are never broadcast; an empty ancestor list is a peer fault (and would
        // otherwise leave the rebuild search with nothing to vary).
        return Err(DecodeError::Malformed("empty ancestor authors".into()));
    }
    let mut seen = vec![false; committee.size()];
    for &author in authors {
        let Some(_) = committee.to_authority_index(author as usize) else {
            return Err(DecodeError::Malformed(format!(
                "ancestor author {author} out of range"
            )));
        };
        if std::mem::replace(&mut seen[author as usize], true) {
            return Err(DecodeError::Malformed(format!(
                "duplicate ancestor author {author}"
            )));
        }
    }
    if let Some(&first) = authors.first()
        && first as usize != stripped_block.author().value()
    {
        return Err(DecodeError::Malformed(
            "first ancestor must be the block author".into(),
        ));
    }
    let mut overridden = vec![false; committee.size()];
    for o in &slim.overrides {
        if o.author as usize >= committee.size() || !seen[o.author as usize] {
            return Err(DecodeError::Malformed(format!(
                "override for non-ancestor author {}",
                o.author
            )));
        }
        if std::mem::replace(&mut overridden[o.author as usize], true) {
            return Err(DecodeError::Malformed(format!(
                "duplicate override for author {}",
                o.author
            )));
        }
        if let Some(round) = o.round
            && round >= stripped_block.round()
        {
            return Err(DecodeError::Malformed(format!(
                "override round {round} not below block round"
            )));
        }
        if let Some(digest) = &o.digest
            && digest.len() != DIGEST_LENGTH
        {
            return Err(DecodeError::Malformed(format!(
                "override digest must be {DIGEST_LENGTH} bytes"
            )));
        }
    }

    let block_ref = BlockRef::new(
        stripped_block.round(),
        stripped_block.author(),
        claimed_digest,
    );
    // Only what the later phases read: the encoded payload is superseded by the decoded
    // block, and the claimed digest is already extracted above.
    Ok(ParsedEnvelope {
        stripped_block,
        ancestor_authors: slim.ancestor_authors,
        overrides: slim.overrides,
        signature: slim.signature,
        block_ref,
        claimed_digest,
    })
}

/// Resolution phase: turns every ancestor into an exact `BlockRef` by reading local
/// state through `resolver`. Kept separate from the rebuild so a caller can scope its
/// state lock to this phase; the reserialization and hashing that follow need no state.
pub(crate) fn resolve_ancestors(
    parsed: &ParsedEnvelope,
    committee: &Committee,
    resolver: &impl AncestorDigestResolver,
) -> Result<Vec<BlockRef>, DecodeError> {
    let ParsedEnvelope {
        stripped_block,
        ancestor_authors,
        overrides,
        block_ref,
        ..
    } = parsed;
    let block_ref = *block_ref;
    let authors = ancestor_authors;
    let parent_round = stripped_block.round().saturating_sub(1);

    // Each ancestor must resolve to exactly one digest. A slot the sender wrote
    // explicitly already has one; an omitted slot must be unique in local state.
    //
    // Anything else falls back to fetching the full block. That is deliberate for
    // equivocation: guessing between candidates would mean rebuilding and hashing once
    // per combination, which an equivocating peer chooses the size of. The fallback
    // costs one full block, which is what propagation cost before this existed, and a
    // validator equivocating often enough to matter is a problem for ancestor selection
    // rather than for the codec.
    let mut ancestors = Vec::with_capacity(authors.len());
    let mut missing: Vec<Slot> = Vec::new();
    for &author in authors {
        let authority = committee
            .to_authority_index(author as usize)
            .expect("validated above");
        let ancestor_override = overrides.iter().find(|o| o.author == author);
        let round = ancestor_override
            .and_then(|o| o.round)
            .unwrap_or(parent_round);
        let slot = Slot::new(round, authority);
        match ancestor_override.and_then(|o| o.digest.as_ref()) {
            Some(digest) => ancestors.push(BlockRef::new(
                round,
                authority,
                BlockDigest(digest.as_ref().try_into().expect("validated above")),
            )),
            None => match resolver.digest_at_slot(slot) {
                SlotDigest::Unique(digest) => {
                    ancestors.push(BlockRef::new(round, authority, digest))
                }
                SlotDigest::Absent => missing.push(slot),
                // Equivocation cannot be cured by waiting, so it outranks any missing
                // slots collected so far.
                SlotDigest::Equivocated => {
                    return Err(DecodeError::NeedFullBlock {
                        block_ref,
                        reason: FallbackReason::AmbiguousSlot(slot),
                    });
                }
            },
        }
    }
    if !missing.is_empty() {
        return Err(DecodeError::NeedFullBlock {
            block_ref,
            reason: FallbackReason::MissingAncestors(missing),
        });
    }
    Ok(ancestors)
}

/// Reassembles the author's exact serialization from the resolved ancestors and accepts
/// it only if it hashes to the digest the sender claimed. Touches no local state.
pub(crate) fn rebuild_from_slim(
    parsed: ParsedEnvelope,
    ancestors: Vec<BlockRef>,
) -> Result<(SignedBlock, Bytes), DecodeError> {
    let ParsedEnvelope {
        stripped_block,
        signature,
        block_ref,
        claimed_digest,
        ..
    } = parsed;

    let signed_block = SignedBlock::from_parts(stripped_block.with_ancestors(ancestors), signature);
    let serialized_block = signed_block
        .serialize()
        .map_err(|e| DecodeError::Malformed(format!("bcs encode: {e}")))?;
    // The author's claimed digest is the arbiter: bytes that do not hash to it are not
    // the block that was signed, whatever local state suggested.
    if VerifiedBlock::compute_digest(&serialized_block) != claimed_digest {
        return Err(DecodeError::NeedFullBlock {
            block_ref,
            reason: FallbackReason::DigestMismatch,
        });
    }
    Ok((signed_block, serialized_block))
}

/// The slim-block format bound to this node: its committee, its genesis
/// digests, and the local state that omitted ancestor digests resolve against.
pub(crate) struct SlimBlockCodec {
    context: Arc<Context>,
    /// Genesis digests by authority index. DagState keeps genesis outside the
    /// slot-indexed blocks, so a round-0 slot resolves to nothing there.
    genesis_digests: Vec<BlockDigest>,
}

struct LocalStateResolver<'a> {
    genesis_digests: &'a [BlockDigest],
    dag_state: &'a DagState,
}

impl AncestorDigestResolver for LocalStateResolver<'_> {
    fn digest_at_slot(&self, slot: Slot) -> SlotDigest {
        if slot.round == GENESIS_ROUND {
            return SlotDigest::Unique(self.genesis_digests[slot.authority.value()]);
        }
        self.dag_state.digest_at_slot(slot)
    }
}

impl SlimBlockCodec {
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

    /// Encodes a block into its slim wire form, omitting the ancestor digests a
    /// receiver can resolve for itself.
    pub(crate) fn encode(
        &self,
        block: &VerifiedBlock,
        dag_state: &RwLock<DagState>,
    ) -> Result<Bytes, bcs::Error> {
        let min_omittable_round = block
            .round()
            .saturating_sub(self.context.protocol_config.gc_depth());
        // The guard covers only the ancestor lookups: cloning the block and serializing
        // it are a full pass over the payload, and holding the DAG lock across them
        // would put block size on the critical path of every accept and commit.
        let overrides = {
            let dag_state = dag_state.read();
            let resolver = LocalStateResolver {
                genesis_digests: &self.genesis_digests,
                dag_state: &dag_state,
            };
            ancestor_overrides(block, &resolver, min_omittable_round)
        };
        serialize_slim(block, overrides)
    }

    /// Rebuilds a slim block into the author's exact serialization, in one
    /// attempt against current state. `author` is the peer the bytes arrived from;
    /// a block claiming any other author is rejected before any state access.
    pub(crate) fn decode(
        &self,
        slim: &[u8],
        author: AuthorityIndex,
        dag_state: &RwLock<DagState>,
    ) -> Result<(SignedBlock, Bytes), DecodeError> {
        let parsed = parse_slim(slim, &self.context.committee, author)?;
        // The guard covers ancestor resolution and nothing else: the reserialize and
        // hash below are a full pass over the block, and holding the DAG lock across
        // them would put block size on the critical path of every accept and commit.
        let ancestors = {
            let dag_state = dag_state.read();
            let resolver = LocalStateResolver {
                genesis_digests: &self.genesis_digests,
                dag_state: &dag_state,
            };
            resolve_ancestors(&parsed, &self.context.committee, &resolver)?
        };
        rebuild_from_slim(parsed, ancestors)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use consensus_config::AuthorityIndex;
    use rand::{RngCore as _, SeedableRng as _, rngs::StdRng};

    use super::*;
    use crate::{
        block::{BlockV1, BlockV3, TestBlock, Transaction},
        storage::mem_store::MemStore,
    };

    /// Both phases in one call. Production drives them from `SlimBlockCodec::decode`,
    /// which parses before taking the state lock the rebuild needs.
    fn deserialize_slim(
        serialized: &[u8],
        committee: &Committee,
        expected_author: AuthorityIndex,
        resolver: &impl AncestorDigestResolver,
    ) -> Result<(SignedBlock, Bytes), DecodeError> {
        let parsed = parse_slim(serialized, committee, expected_author)?;
        let ancestors = resolve_ancestors(&parsed, committee, resolver)?;
        rebuild_from_slim(parsed, ancestors)
    }

    #[derive(Default, Clone)]
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
        fn digest_at_slot(&self, slot: Slot) -> SlotDigest {
            match self.0.get(&slot).map(Vec::as_slice) {
                None | Some([]) => SlotDigest::Absent,
                Some([digest]) => SlotDigest::Unique(*digest),
                Some(_) => SlotDigest::Equivocated,
            }
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

    /// Both encoding halves in one call, as production drives them from
    /// `SlimBlockCodec::encode` (which scopes its state lock to the first).
    fn serialize_slim(
        block: &VerifiedBlock,
        resolver: &impl AncestorDigestResolver,
        min_omittable_round: Round,
    ) -> Result<Bytes, bcs::Error> {
        super::serialize_slim(
            block,
            super::ancestor_overrides(block, resolver, min_omittable_round),
        )
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
        let slim = serialize_slim(block, resolver, min_omittable_round).unwrap();
        let (signed, serialized) =
            deserialize_slim(&slim, &context.committee, block.author(), resolver).unwrap();
        assert_eq!(&serialized, block.serialized());
        assert_eq!(VerifiedBlock::compute_digest(&serialized), block.digest());
        signed.verify_signature(context).unwrap();
        slim
    }

    /// Byte-identical rebuild across every block version, with the common-case
    /// wire shape pinned once: the only override is the author's own parent (its
    /// digest always rides explicitly) and the slim form is strictly smaller.
    #[tokio::test]
    async fn roundtrip_all_block_versions() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(7);
        for version in 1..=3u32 {
            let mut resolver = MapResolver::default();
            let ancestors = ancestor_refs(4, 9, &mut resolver, &mut rng);
            let transactions = vec![Transaction::new(vec![1, 2, 3])];
            let author = AuthorityIndex::new_for_test(0);
            let block = match version {
                1 => Block::V1(BlockV1::new(
                    0,
                    10,
                    author,
                    1000,
                    ancestors,
                    transactions,
                    vec![],
                    vec![],
                )),
                2 => TestBlock::new(10, 0)
                    .set_ancestors_raw(ancestors)
                    .set_transactions(transactions)
                    .build(),
                _ => Block::V3(BlockV3::new(
                    0,
                    10,
                    author,
                    1000,
                    ancestors,
                    transactions,
                    vec![],
                    5,
                    vec![],
                    vec![],
                )),
            };
            let block = sign(block, &context, &key_pairs);
            let slim = roundtrip(&block, &context, &resolver, 0);
            let decoded = SlimBlock::decode(slim.as_ref()).unwrap();
            assert_eq!(decoded.overrides.len(), 1, "version {version}");
            assert_eq!(decoded.overrides[0].author, 0);
            assert!(decoded.overrides[0].digest.is_some());
            assert!(slim.len() < block.serialized().len(), "version {version}");
        }
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
        let slim = roundtrip(&block, &context, &resolver, 16);
        let decoded = SlimBlock::decode(slim.as_ref()).unwrap();
        assert_eq!(decoded.overrides.len(), 3);
        assert!(decoded.overrides.iter().all(|o| o.digest.is_some()));
    }

    /// The proposer emits authority-ordered ancestors and then APPENDS the ones it
    /// promotes back in to reach the parent-round quorum, in score order. That tail is
    /// not derivable from the committee, which is why `ancestor_authors` is an ordered
    /// list rather than a membership set.
    ///
    /// Every other test happens to build ancestors in authority order, where a decoder
    /// that sorted would be indistinguishable from one that preserved order. This one
    /// is not: it fails if the order is not carried and restored exactly.
    #[tokio::test]
    async fn promoted_ancestors_keep_their_proposal_order() {
        let (context, key_pairs) = Context::new_for_test(6);
        let mut rng = StdRng::seed_from_u64(41);
        let mut resolver = MapResolver::default();
        let refs = ancestor_refs(6, 9, &mut resolver, &mut rng);

        // Own parent first, then the high scorers, then authorities 2 and 4 promoted
        // back in on score - so the vector is deliberately not authority-sorted.
        let ancestors = vec![refs[0], refs[1], refs[3], refs[5], refs[4], refs[2]];
        let authors: Vec<u32> = ancestors.iter().map(|r| r.author.value() as u32).collect();
        assert!(
            authors.windows(2).any(|w| w[0] > w[1]),
            "the fixture must not be authority-sorted, or it proves nothing"
        );

        let block = TestBlock::new(10, 0).set_ancestors_raw(ancestors).build();
        let block = sign(block, &context, &key_pairs);

        roundtrip(&block, &context, &resolver, 0);

        let decoded =
            SlimBlock::decode(serialize_slim(&block, &resolver, 0).unwrap().as_ref()).unwrap();
        assert_eq!(
            decoded.ancestor_authors, authors,
            "the wire must carry proposal order, not authority order"
        );
    }

    /// The horizon is exclusive: an ancestor sitting exactly on `min_omittable_round`
    /// keeps its digest, and only strictly newer rounds are omitted.
    ///
    /// The direction is load-bearing and its failure is silent. A receiver skips any
    /// block at or below its own gc_round without storing it, and for a receiver in
    /// sync that round IS the horizon -- so omitting the digest there points at the one
    /// slot guaranteed absent, costing a full-block fetch with nothing logged. The
    /// other horizon test uses rounds well clear of the boundary and would pass either
    /// way, so this is what pins the comparison.
    #[tokio::test]
    async fn ancestor_on_the_horizon_keeps_its_digest() {
        let (context, key_pairs) = Context::new_for_test(4);
        let mut rng = StdRng::seed_from_u64(53);
        let horizon = 16;
        let mut resolver = MapResolver::default();

        // Authority 1 sits exactly on the horizon, authority 2 one round above it.
        let mut ancestors = ancestor_refs(4, 19, &mut resolver, &mut rng);
        ancestors[1] = BlockRef::new(
            horizon,
            AuthorityIndex::new_for_test(1),
            test_digest(&mut rng),
        );
        ancestors[2] = BlockRef::new(
            horizon + 1,
            AuthorityIndex::new_for_test(2),
            test_digest(&mut rng),
        );
        resolver.insert(ancestors[1]);
        resolver.insert(ancestors[2]);

        let block = TestBlock::new(20, 0).set_ancestors_raw(ancestors).build();
        let block = sign(block, &context, &key_pairs);
        let slim = roundtrip(&block, &context, &resolver, horizon);

        let decoded = SlimBlock::decode(slim.as_ref()).unwrap();
        let digest_for = |author: u32| {
            decoded
                .overrides
                .iter()
                .find(|o| o.author == author)
                .and_then(|o| o.digest.as_ref())
        };
        assert!(
            digest_for(1).is_some(),
            "an ancestor at exactly min_omittable_round must keep its digest"
        );
        assert!(
            digest_for(2).is_none(),
            "an ancestor one round above the horizon must have its digest omitted"
        );
    }

    /// A receiver that dropped block X from an author must still decode the author's next
    /// block X': the own-parent digest rides explicitly, so decoding never needs to resolve
    /// the one slot the receiver is missing, and the rebuilt ancestors carry X's exact
    /// ref for block_manager to suspend on and sync to fetch. Without this, dropping one
    /// block would make every later block from that author undecodable.
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
        let slim = serialize_slim(&x2, &sender, 0).unwrap();

        let (_signed, serialized) =
            deserialize_slim(&slim, &context.committee, x2.author(), &receiver).unwrap();
        // Byte-identity proves the rebuilt ancestors include X's exact ref.
        assert_eq!(&serialized, x2.serialized());
    }
    /// Any equivocation at an omitted slot falls back rather than guessing. Two
    /// candidates is already ambiguous: choosing between them would mean rebuilding and
    /// hashing per combination, and an equivocating peer picks how many there are.
    #[tokio::test]
    async fn equivocating_slot_falls_back_without_guessing() {
        let (context, key_pairs) = Context::new_for_test(7);
        let mut rng = StdRng::seed_from_u64(42);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(7, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let slim = serialize_slim(&block, &resolver, 0).unwrap();

        // One candidate per slot decodes to the author's exact bytes.
        let (_signed, serialized) =
            deserialize_slim(&slim, &context.committee, block.author(), &resolver).unwrap();
        assert_eq!(&serialized, block.serialized());

        // A second candidate at one slot refuses, even though the correct digest is
        // still present among them.
        let mut equivocating = resolver.clone();
        equivocating.insert(BlockRef::new(9, ancestors[3].author, test_digest(&mut rng)));
        match deserialize_slim(&slim, &context.committee, block.author(), &equivocating).map(|_| ())
        {
            Err(DecodeError::NeedFullBlock { reason, .. }) => {
                assert_eq!(
                    reason,
                    FallbackReason::AmbiguousSlot(Slot::from(ancestors[3]))
                );
            }
            other => panic!("expected AmbiguousSlot fallback, got {other:?}"),
        }
    }

    /// One decode pass reports the COMPLETE missing frontier in canonical ancestor
    /// order — recovery registers on all of it and waits once, so first-missing-only
    /// reporting would reintroduce the wake-per-slot churn measured in v3.
    #[tokio::test]
    async fn missing_frontier_is_complete_and_ordered() {
        let (context, key_pairs) = Context::new_for_test(6);
        let mut rng = StdRng::seed_from_u64(17);
        let mut resolver = MapResolver::default();
        let ancestors = ancestor_refs(6, 9, &mut resolver, &mut rng);
        let block = TestBlock::new(10, 0)
            .set_ancestors_raw(ancestors.clone())
            .build();
        let block = sign(block, &context, &key_pairs);
        let slim = serialize_slim(&block, &resolver, 0).unwrap();

        // Receiver holds only ancestors 1 and 4: the frontier must name 2, 3, and 5
        // (the own parent at index 0 rides an explicit digest), in ancestor order.
        let mut receiver = MapResolver::default();
        receiver.insert(ancestors[1]);
        receiver.insert(ancestors[4]);
        match deserialize_slim(&slim, &context.committee, block.author(), &receiver).map(|_| ()) {
            Err(DecodeError::NeedFullBlock {
                block_ref, reason, ..
            }) => {
                assert_eq!(block_ref, block.reference());
                assert_eq!(
                    reason,
                    FallbackReason::MissingAncestors(vec![
                        Slot::from(ancestors[2]),
                        Slot::from(ancestors[3]),
                        Slot::from(ancestors[5]),
                    ])
                );
            }
            other => panic!("expected MissingAncestors fallback, got {other:?}"),
        }

        // Ambiguity at any slot wins over missing slots: waiting cures neither.
        let mut flooded = MapResolver::default();
        flooded.insert(ancestors[1]);
        flooded.insert(ancestors[4]);
        flooded.insert(BlockRef::new(9, ancestors[4].author, test_digest(&mut rng)));
        match deserialize_slim(&slim, &context.committee, block.author(), &flooded).map(|_| ()) {
            Err(DecodeError::NeedFullBlock { reason, .. }) => {
                assert!(matches!(reason, FallbackReason::AmbiguousSlot(_)));
            }
            other => panic!("expected AmbiguousSlot fallback, got {other:?}"),
        }
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
        let slim = serialize_slim(&block, &resolver, 0).unwrap();

        // Receiver holds a *different* single block at one slot (asymmetric equivocation):
        // the rebuild succeeds but produces the wrong bytes => DigestMismatch.
        let mut skewed = MapResolver::default();
        for ancestor in &ancestors[..3] {
            skewed.insert(*ancestor);
        }
        skewed.insert(BlockRef::new(9, ancestors[3].author, test_digest(&mut rng)));
        match deserialize_slim(&slim, &context.committee, block.author(), &skewed).map(|_| ()) {
            Err(DecodeError::NeedFullBlock {
                block_ref, reason, ..
            }) => {
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
        let slim = serialize_slim(&block, &resolver, 0).unwrap();
        let committee = &context.committee;

        // Garbage bytes.
        assert!(matches!(
            deserialize_slim(b"garbage", committee, block.author(), &resolver),
            Err(DecodeError::Malformed(_))
        ));

        let decoded = SlimBlock::decode(slim.as_ref()).unwrap();
        let tamper = |f: &dyn Fn(&mut SlimBlock)| {
            let mut m = decoded.clone();
            f(&mut m);
            deserialize_slim(&m.encode_to_vec(), committee, block.author(), &resolver)
        };

        // Tampered claimed digest => mismatch fallback, not acceptance.
        assert!(matches!(
            tamper(&|m| {
                let mut digest = m.claimed_block_digest.to_vec();
                digest[0] ^= 1;
                m.claimed_block_digest = digest.into();
            }),
            Err(DecodeError::NeedFullBlock {
                reason: FallbackReason::DigestMismatch,
                ..
            })
        ));
        // Bad digest length.
        assert!(matches!(
            tamper(&|m| m.claimed_block_digest = Bytes::from_static(&[1, 2, 3])),
            Err(DecodeError::Malformed(_))
        ));
        // Override digest of the wrong length. prost accepts a bytes field of any size,
        // so the length check in `parse_slim` is the only thing standing in front of
        // the `try_into().expect(...)` in `resolve_ancestors`.
        assert!(matches!(
            tamper(&|m| m.overrides[0].digest = Some(Bytes::from_static(&[9u8; 31]))),
            Err(DecodeError::Malformed(_))
        ));
        // Empty ancestor list (would otherwise reach the rebuild search with
        // nothing to vary — regression guard for a remote panic).
        assert!(matches!(
            tamper(&|m| {
                m.ancestor_authors.clear();
                m.overrides.clear();
            }),
            Err(DecodeError::Malformed(_))
        ));
        // Duplicate ancestor author.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors[2] = m.ancestor_authors[1]),
            Err(DecodeError::Malformed(_))
        ));
        // Author out of committee range.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors[2] = 100),
            Err(DecodeError::Malformed(_))
        ));
        // More authors than the committee.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors = (0..10).collect()),
            Err(DecodeError::Malformed(_))
        ));
        // First ancestor not the block author.
        assert!(matches!(
            tamper(&|m| m.ancestor_authors.swap(0, 1)),
            Err(DecodeError::Malformed(_))
        ));
        // Override for a non-ancestor author.
        assert!(matches!(
            tamper(&|m| m.overrides.push(AncestorOverride {
                author: 50,
                round: None,
                digest: None
            })),
            Err(DecodeError::Malformed(_))
        ));
        // Override round not below the block round.
        assert!(matches!(
            tamper(&|m| m.overrides.push(AncestorOverride {
                author: 1,
                round: Some(10),
                digest: None
            })),
            Err(DecodeError::Malformed(_))
        ));
        // Skeleton with non-empty ancestors (would double-count on splice).
        assert!(matches!(
            tamper(&|m| {
                let stripped_block: Block = bcs::from_bytes(&m.block).unwrap();
                let stripped = stripped_block.with_ancestors(vec![BlockRef::MIN]);
                m.block = bcs::to_bytes(&stripped).unwrap().into();
            }),
            Err(DecodeError::Malformed(_))
        ));
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
        let slim = serialize_slim(&block, &resolver, 0).unwrap();

        // The subscription stream only carries the sending peer's own proposals: a block
        // claiming a different author is a peer fault, rejected without slot lookups.
        assert!(matches!(
            deserialize_slim(
                &slim,
                &context.committee,
                AuthorityIndex::new_for_test(1),
                &resolver,
            )
            .map(|_| ()),
            Err(DecodeError::Malformed(_))
        ));

        // Same for a block from another epoch.
        let stale = TestBlock::new(10, 0)
            .set_epoch(7)
            .set_ancestors_raw(ancestors)
            .build();
        let stale = sign(stale, &context, &key_pairs);
        let stale_minimal = serialize_slim(&stale, &resolver, 0).unwrap();
        assert!(matches!(
            deserialize_slim(
                &stale_minimal,
                &context.committee,
                stale.author(),
                &resolver,
            )
            .map(|_| ()),
            Err(DecodeError::Malformed(_))
        ));
    }

    fn test_context() -> Arc<Context> {
        let (context, _keys) = Context::new_for_test(4);
        Arc::new(context)
    }

    fn empty_dag(context: &Arc<Context>) -> Arc<RwLock<DagState>> {
        Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )))
    }

    /// Builds a round-2 block over the four round-1 blocks, accepting the ancestors
    /// into `dag` so a sender can omit their digests.
    fn block_over_round_one(context: &Arc<Context>, dag: &Arc<RwLock<DagState>>) -> VerifiedBlock {
        let genesis: Vec<_> = genesis_blocks(context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let mut ancestors = Vec::new();
        for authority in 0..4u32 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(1, authority)
                    .set_ancestors_raw(genesis.clone())
                    .build(),
            );
            dag.write().accept_block(block.clone());
            ancestors.push(block.reference());
        }
        ancestors.sort_by_key(|r| (r.author.value() != 0, r.author));
        VerifiedBlock::new_for_test(TestBlock::new(2, 0).set_ancestors_raw(ancestors).build())
    }

    /// The property the receive path rests on: a real block, stripped and rebuilt,
    /// reproduces the author's exact signed bytes. Ground truth is the original
    /// block, so this holds the codec to the block rather than to itself.
    #[tokio::test]
    async fn stripping_and_rebuilding_reproduces_the_original_bytes() {
        let context = test_context();
        let dag = empty_dag(&context);
        let block = block_over_round_one(&context, &dag);

        let codec = SlimBlockCodec::new(context.clone());
        let slim = codec.encode(&block, &dag).unwrap();
        let (_signed, serialized) = codec.decode(&slim, block.author(), &dag).unwrap();

        assert_eq!(&serialized, block.serialized());
    }

    /// Genesis ancestors resolve from the committee rather than the DAG, so a
    /// round-1 block rebuilds against a receiver that has accepted nothing.
    #[tokio::test]
    async fn genesis_ancestors_rebuild_without_any_accepted_block() {
        let context = test_context();
        let genesis: Vec<_> = genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let block =
            VerifiedBlock::new_for_test(TestBlock::new(1, 0).set_ancestors_raw(genesis).build());

        let codec = SlimBlockCodec::new(context.clone());
        let sender_dag = empty_dag(&context);
        let slim = codec.encode(&block, &sender_dag).unwrap();

        let receiver_dag = empty_dag(&context);
        let (_signed, serialized) = codec.decode(&slim, block.author(), &receiver_dag).unwrap();

        assert_eq!(&serialized, block.serialized());
    }

    /// An ancestor the receiver has not accepted cannot be resolved, so decoding
    /// reports it as unresolvable rather than guessing a digest. What the caller
    /// then does with it is the caller's decision.
    #[tokio::test]
    async fn unresolvable_ancestor_is_reported_not_guessed() {
        let context = test_context();
        let sender_dag = empty_dag(&context);
        let block = block_over_round_one(&context, &sender_dag);

        let codec = SlimBlockCodec::new(context.clone());
        let slim = codec.encode(&block, &sender_dag).unwrap();

        let receiver_dag = empty_dag(&context);
        let result = codec
            .decode(&slim, block.author(), &receiver_dag)
            .map(|_| ());

        match result {
            Err(DecodeError::NeedFullBlock {
                reason: FallbackReason::MissingAncestors(missing),
                ..
            }) => assert_eq!(
                missing,
                block.ancestors()[1..]
                    .iter()
                    .map(|a| Slot::from(*a))
                    .collect::<Vec<_>>(),
                "every unresolvable slot must be reported, in ancestor order"
            ),
            other => panic!("expected MissingAncestors, got {other:?}"),
        }
    }

    /// A block claiming an author other than the peer it arrived from is rejected,
    /// so a peer cannot use the stream to introduce another authority's blocks.
    #[tokio::test]
    async fn author_must_match_the_sending_peer() {
        let context = test_context();
        let dag = empty_dag(&context);
        let block = block_over_round_one(&context, &dag);

        let codec = SlimBlockCodec::new(context.clone());
        let slim = codec.encode(&block, &dag).unwrap();

        // Malformed, not NeedFullBlock: the peer is at fault, and `metric_label`
        // reports it as such rather than as local state being behind.
        let other_peer = context.committee.to_authority_index(1).unwrap();
        assert!(matches!(
            codec.decode(&slim, other_peer, &dag),
            Err(DecodeError::Malformed(_))
        ));
    }
}
