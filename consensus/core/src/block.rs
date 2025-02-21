// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

use bytes::Bytes;
use consensus_config::{
    AuthorityIndex, DefaultHashFunction, Epoch, ProtocolKeyPair, ProtocolKeySignature,
    ProtocolPublicKey, DIGEST_LENGTH,
};
use enum_dispatch::enum_dispatch;
use fastcrypto::hash::{Digest, HashFunction};
use serde::{Deserialize, Serialize};
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};

use crate::{
    commit::CommitVote,
    context::Context,
    ensure,
    error::{ConsensusError, ConsensusResult},
};

/// Round number of a block.
pub type Round = u32;

pub(crate) const GENESIS_ROUND: Round = 0;

/// Block proposal timestamp in milliseconds.
pub type BlockTimestampMs = u64;

/// Sui transaction in serialised bytes
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Default, Debug)]
pub struct Transaction {
    data: Bytes,
}

impl Transaction {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data: data.into() }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Bytes {
        self.data
    }
}

/// Index of a transaction in a block.
pub type TransactionIndex = u16;

/// Votes on transactions in a specific block.
/// Reject votes are explicit. The rest of transactions in the block receive implicit accept votes.
// TODO: look into making fields `pub`.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct BlockTransactionVotes {
    pub(crate) block_ref: BlockRef,
    pub(crate) rejects: Vec<TransactionIndex>,
}

/// A block includes references to previous round blocks and transactions that the authority
/// considers valid.
/// Well behaved authorities produce at most one block per round, but malicious authorities can
/// equivocate.
#[allow(private_interfaces)]
#[derive(Clone, Deserialize, Serialize)]
#[enum_dispatch(BlockAPI)]
pub enum Block {
    V1(BlockV1),
    V2(BlockV2),
}

#[allow(private_interfaces)]
#[enum_dispatch]
pub trait BlockAPI {
    fn epoch(&self) -> Epoch;
    fn round(&self) -> Round;
    fn author(&self) -> AuthorityIndex;
    fn slot(&self) -> Slot;
    fn timestamp_ms(&self) -> BlockTimestampMs;
    fn ancestors(&self) -> &[BlockRef];
    fn transactions(&self) -> &[Transaction];
    fn transactions_data(&self) -> Vec<&[u8]>;
    fn commit_votes(&self) -> &[CommitVote];
    fn transaction_votes(&self) -> &[BlockTransactionVotes];
    fn misbehavior_reports(&self) -> &[MisbehaviorReport];
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct BlockV1 {
    epoch: Epoch,
    round: Round,
    author: AuthorityIndex,
    timestamp_ms: BlockTimestampMs,
    ancestors: Vec<BlockRef>,
    transactions: Vec<Transaction>,
    commit_votes: Vec<CommitVote>,
    misbehavior_reports: Vec<MisbehaviorReport>,
}

impl BlockV1 {
    pub(crate) fn new(
        epoch: Epoch,
        round: Round,
        author: AuthorityIndex,
        timestamp_ms: BlockTimestampMs,
        ancestors: Vec<BlockRef>,
        transactions: Vec<Transaction>,
        commit_votes: Vec<CommitVote>,
        misbehavior_reports: Vec<MisbehaviorReport>,
    ) -> BlockV1 {
        Self {
            epoch,
            round,
            author,
            timestamp_ms,
            ancestors,
            transactions,
            commit_votes,
            misbehavior_reports,
        }
    }

    fn genesis_block(epoch: Epoch, author: AuthorityIndex) -> Self {
        Self {
            epoch,
            round: GENESIS_ROUND,
            author,
            timestamp_ms: 0,
            ancestors: vec![],
            transactions: vec![],
            commit_votes: vec![],
            misbehavior_reports: vec![],
        }
    }
}

impl BlockAPI for BlockV1 {
    fn epoch(&self) -> Epoch {
        self.epoch
    }

    fn round(&self) -> Round {
        self.round
    }

    fn author(&self) -> AuthorityIndex {
        self.author
    }

    fn slot(&self) -> Slot {
        Slot::new(self.round, self.author)
    }

    fn timestamp_ms(&self) -> BlockTimestampMs {
        self.timestamp_ms
    }

    fn ancestors(&self) -> &[BlockRef] {
        &self.ancestors
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn transactions_data(&self) -> Vec<&[u8]> {
        self.transactions.iter().map(|t| t.data()).collect()
    }

    fn commit_votes(&self) -> &[CommitVote] {
        &self.commit_votes
    }

    fn transaction_votes(&self) -> &[BlockTransactionVotes] {
        &[]
    }

    fn misbehavior_reports(&self) -> &[MisbehaviorReport] {
        &self.misbehavior_reports
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct BlockV2 {
    epoch: Epoch,
    round: Round,
    author: AuthorityIndex,
    timestamp_ms: BlockTimestampMs,
    ancestors: Vec<BlockRef>,
    transactions: Vec<Transaction>,
    transaction_votes: Vec<BlockTransactionVotes>,
    commit_votes: Vec<CommitVote>,
    misbehavior_reports: Vec<MisbehaviorReport>,
}

#[allow(unused)]
impl BlockV2 {
    pub(crate) fn new(
        epoch: Epoch,
        round: Round,
        author: AuthorityIndex,
        timestamp_ms: BlockTimestampMs,
        ancestors: Vec<BlockRef>,
        transactions: Vec<Transaction>,
        commit_votes: Vec<CommitVote>,
        transaction_votes: Vec<BlockTransactionVotes>,
        misbehavior_reports: Vec<MisbehaviorReport>,
    ) -> BlockV2 {
        Self {
            epoch,
            round,
            author,
            timestamp_ms,
            ancestors,
            transactions,
            commit_votes,
            transaction_votes,
            misbehavior_reports,
        }
    }

    fn genesis_block(epoch: Epoch, author: AuthorityIndex) -> Self {
        Self {
            epoch,
            round: GENESIS_ROUND,
            author,
            timestamp_ms: 0,
            ancestors: vec![],
            transactions: vec![],
            commit_votes: vec![],
            transaction_votes: vec![],
            misbehavior_reports: vec![],
        }
    }
}

impl BlockAPI for BlockV2 {
    fn epoch(&self) -> Epoch {
        self.epoch
    }

    fn round(&self) -> Round {
        self.round
    }

    fn author(&self) -> AuthorityIndex {
        self.author
    }

    fn slot(&self) -> Slot {
        Slot::new(self.round, self.author)
    }

    fn timestamp_ms(&self) -> BlockTimestampMs {
        self.timestamp_ms
    }

    fn ancestors(&self) -> &[BlockRef] {
        &self.ancestors
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn transactions_data(&self) -> Vec<&[u8]> {
        self.transactions.iter().map(|t| t.data()).collect()
    }

    fn transaction_votes(&self) -> &[BlockTransactionVotes] {
        &self.transaction_votes
    }

    fn commit_votes(&self) -> &[CommitVote] {
        &self.commit_votes
    }

    fn misbehavior_reports(&self) -> &[MisbehaviorReport] {
        &self.misbehavior_reports
    }
}

/// `BlockRef` uniquely identifies a `VerifiedBlock` via `digest`. It also contains the slot
/// info (round and author) so it can be used in logic such as aggregating stakes for a round.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockRef {
    pub round: Round,
    pub author: AuthorityIndex,
    pub digest: BlockDigest,
}

impl BlockRef {
    pub const MIN: Self = Self {
        round: 0,
        author: AuthorityIndex::MIN,
        digest: BlockDigest::MIN,
    };

    pub const MAX: Self = Self {
        round: u32::MAX,
        author: AuthorityIndex::MAX,
        digest: BlockDigest::MAX,
    };

    pub fn new(round: Round, author: AuthorityIndex, digest: BlockDigest) -> Self {
        Self {
            round,
            author,
            digest,
        }
    }
}

// TODO: re-evaluate formats for production debugging.
impl fmt::Display for BlockRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "B{}({},{})", self.round, self.author, self.digest)
    }
}

impl fmt::Debug for BlockRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "B{}({},{:?})", self.round, self.author, self.digest)
    }
}

impl Hash for BlockRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.digest.0[..8]);
    }
}

/// Digest of a `VerifiedBlock` or verified `SignedBlock`, which covers the `Block` and its
/// signature.
///
/// Note: the signature algorithm is assumed to be non-malleable, so it is impossible for another
/// party to create an altered but valid signature, producing an equivocating `BlockDigest`.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockDigest([u8; consensus_config::DIGEST_LENGTH]);

impl BlockDigest {
    /// Lexicographic min & max digest.
    pub const MIN: Self = Self([u8::MIN; consensus_config::DIGEST_LENGTH]);
    pub const MAX: Self = Self([u8::MAX; consensus_config::DIGEST_LENGTH]);
}

impl Hash for BlockDigest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0[..8]);
    }
}

impl From<BlockDigest> for Digest<{ DIGEST_LENGTH }> {
    fn from(hd: BlockDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Display for BlockDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..4)
                .ok_or(fmt::Error)?
        )
    }
}

impl fmt::Debug for BlockDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

impl AsRef<[u8]> for BlockDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Slot is the position of blocks in the DAG. It can contain 0, 1 or multiple blocks
/// from the same authority at the same round.
#[derive(Clone, Copy, PartialEq, PartialOrd, Default, Hash)]
pub struct Slot {
    pub round: Round,
    pub authority: AuthorityIndex,
}

impl Slot {
    pub fn new(round: Round, authority: AuthorityIndex) -> Self {
        Self { round, authority }
    }

    #[cfg(test)]
    pub fn new_for_test(round: Round, authority: u32) -> Self {
        Self {
            round,
            authority: AuthorityIndex::new_for_test(authority),
        }
    }
}

impl From<BlockRef> for Slot {
    fn from(value: BlockRef) -> Self {
        Slot::new(value.round, value.author)
    }
}

// TODO: re-evaluate formats for production debugging.
impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.authority, self.round,)
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// A Block with its signature, before they are verified.
///
/// Note: `BlockDigest` is computed over this struct, so any added field (without `#[serde(skip)]`)
/// will affect the values of `BlockDigest` and `BlockRef`.
#[derive(Deserialize, Serialize)]
pub(crate) struct SignedBlock {
    inner: Block,
    signature: Bytes,
}

impl SignedBlock {
    /// Should only be used when constructing the genesis blocks
    pub(crate) fn new_genesis(block: Block) -> Self {
        Self {
            inner: block,
            signature: Bytes::default(),
        }
    }

    pub(crate) fn new(block: Block, protocol_keypair: &ProtocolKeyPair) -> ConsensusResult<Self> {
        let signature = compute_block_signature(&block, protocol_keypair)?;
        Ok(Self {
            inner: block,
            signature: Bytes::copy_from_slice(signature.to_bytes()),
        })
    }

    pub(crate) fn signature(&self) -> &Bytes {
        &self.signature
    }

    /// This method only verifies this block's signature. Verification of the full block
    /// should be done via BlockVerifier.
    pub(crate) fn verify_signature(&self, context: &Context) -> ConsensusResult<()> {
        let block = &self.inner;
        let committee = &context.committee;
        ensure!(
            committee.is_valid_index(block.author()),
            ConsensusError::InvalidAuthorityIndex {
                index: block.author(),
                max: committee.size() - 1
            }
        );
        let authority = committee.authority(block.author());
        verify_block_signature(block, self.signature(), &authority.protocol_key)
    }

    /// Serialises the block using the bcs serializer
    pub(crate) fn serialize(&self) -> Result<Bytes, bcs::Error> {
        let bytes = bcs::to_bytes(self)?;
        Ok(bytes.into())
    }

    /// Clears signature for testing.
    #[cfg(test)]
    pub(crate) fn clear_signature(&mut self) {
        self.signature = Bytes::default();
    }
}

/// Digest of a block, covering all `Block` fields without its signature.
/// This is used during Block signing and signature verification.
/// This should never be used outside of this file, to avoid confusion with `BlockDigest`.
#[derive(Serialize, Deserialize)]
struct InnerBlockDigest([u8; consensus_config::DIGEST_LENGTH]);

/// Computes the digest of a Block, only for signing and verifications.
fn compute_inner_block_digest(block: &Block) -> ConsensusResult<InnerBlockDigest> {
    let mut hasher = DefaultHashFunction::new();
    hasher.update(bcs::to_bytes(block).map_err(ConsensusError::SerializationFailure)?);
    Ok(InnerBlockDigest(hasher.finalize().into()))
}

/// Wrap a InnerBlockDigest in the intent message.
fn to_consensus_block_intent(digest: InnerBlockDigest) -> IntentMessage<InnerBlockDigest> {
    IntentMessage::new(Intent::consensus_app(IntentScope::ConsensusBlock), digest)
}

/// Process for signing a block & verifying a block signature:
/// 1. Compute the digest of `Block`.
/// 2. Wrap the digest in `IntentMessage`.
/// 3. Sign the serialized `IntentMessage`, or verify signature against it.
fn compute_block_signature(
    block: &Block,
    protocol_keypair: &ProtocolKeyPair,
) -> ConsensusResult<ProtocolKeySignature> {
    let digest = compute_inner_block_digest(block)?;
    let message = bcs::to_bytes(&to_consensus_block_intent(digest))
        .map_err(ConsensusError::SerializationFailure)?;
    Ok(protocol_keypair.sign(&message))
}

fn verify_block_signature(
    block: &Block,
    signature: &[u8],
    protocol_pubkey: &ProtocolPublicKey,
) -> ConsensusResult<()> {
    let digest = compute_inner_block_digest(block)?;
    let message = bcs::to_bytes(&to_consensus_block_intent(digest))
        .map_err(ConsensusError::SerializationFailure)?;
    let sig =
        ProtocolKeySignature::from_bytes(signature).map_err(ConsensusError::MalformedSignature)?;
    protocol_pubkey
        .verify(&message, &sig)
        .map_err(ConsensusError::SignatureVerificationFailure)
}

/// Allow quick access on the underlying Block without having to always refer to the inner block ref.
impl Deref for SignedBlock {
    type Target = Block;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// VerifiedBlock allows full access to its content.
/// Note: clone() is relatively cheap with most underlying data refcounted.
#[derive(Clone)]
pub struct VerifiedBlock {
    block: Arc<SignedBlock>,

    // Cached Block digest and serialized SignedBlock, to avoid re-computing these values.
    digest: BlockDigest,
    serialized: Bytes,
}

impl VerifiedBlock {
    /// Creates VerifiedBlock from a verified SignedBlock and its serialized bytes.
    pub(crate) fn new_verified(signed_block: SignedBlock, serialized: Bytes) -> Self {
        let digest = Self::compute_digest(&serialized);
        VerifiedBlock {
            block: Arc::new(signed_block),
            digest,
            serialized,
        }
    }

    /// This method is public for testing in other crates.
    pub fn new_for_test(block: Block) -> Self {
        // Use empty signature in test.
        let signed_block = SignedBlock {
            inner: block,
            signature: Default::default(),
        };
        let serialized: Bytes = bcs::to_bytes(&signed_block)
            .expect("Serialization should not fail")
            .into();
        let digest = Self::compute_digest(&serialized);
        VerifiedBlock {
            block: Arc::new(signed_block),
            digest,
            serialized,
        }
    }

    /// Returns reference to the block.
    pub fn reference(&self) -> BlockRef {
        BlockRef {
            round: self.round(),
            author: self.author(),
            digest: self.digest(),
        }
    }

    pub(crate) fn digest(&self) -> BlockDigest {
        self.digest
    }

    /// Returns the serialized block with signature.
    pub(crate) fn serialized(&self) -> &Bytes {
        &self.serialized
    }

    /// Computes digest from the serialized block with signature.
    pub(crate) fn compute_digest(serialized: &[u8]) -> BlockDigest {
        let mut hasher = DefaultHashFunction::new();
        hasher.update(serialized);
        BlockDigest(hasher.finalize().into())
    }
}

/// Allow quick access on the underlying Block without having to always refer to the inner block ref.
impl Deref for VerifiedBlock {
    type Target = Block;

    fn deref(&self) -> &Self::Target {
        &self.block.inner
    }
}

impl PartialEq for VerifiedBlock {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

impl fmt::Display for VerifiedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.reference())
    }
}

// TODO: re-evaluate formats for production debugging.
impl fmt::Debug for VerifiedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{:?}({}ms;{:?};{}t;{}c)",
            self.reference(),
            self.timestamp_ms(),
            self.ancestors(),
            self.transactions().len(),
            self.commit_votes().len(),
        )
    }
}

/// Block with extended additional information, such as
/// local blocks that are excluded from the block's ancestors.
/// The extended information do not need to be certified or forwarded to other authorities.
#[derive(Clone, Debug)]
pub(crate) struct ExtendedBlock {
    pub block: VerifiedBlock,
    pub excluded_ancestors: Vec<BlockRef>,
}

/// Generates the genesis blocks for the current Committee.
/// The blocks are returned in authority index order.
pub(crate) fn genesis_blocks(context: Arc<Context>) -> Vec<VerifiedBlock> {
    context
        .committee
        .authorities()
        .map(|(authority_index, _)| {
            let signed_block = SignedBlock::new_genesis(Block::V1(BlockV1::genesis_block(
                context.committee.epoch(),
                authority_index,
            )));
            let serialized = signed_block
                .serialize()
                .expect("Genesis block serialization failed.");
            // Unnecessary to verify genesis blocks.
            VerifiedBlock::new_verified(signed_block, serialized)
        })
        .collect::<Vec<VerifiedBlock>>()
}

/// Creates fake blocks for testing.
/// This struct is public for testing in other crates.
#[derive(Clone)]
pub struct TestBlock {
    block: BlockV1,
}

impl TestBlock {
    pub fn new(round: Round, author: u32) -> Self {
        Self {
            block: BlockV1 {
                round,
                author: AuthorityIndex::new_for_test(author),
                ..Default::default()
            },
        }
    }

    pub fn set_epoch(mut self, epoch: Epoch) -> Self {
        self.block.epoch = epoch;
        self
    }

    pub fn set_round(mut self, round: Round) -> Self {
        self.block.round = round;
        self
    }

    pub fn set_author(mut self, author: AuthorityIndex) -> Self {
        self.block.author = author;
        self
    }

    pub fn set_timestamp_ms(mut self, timestamp_ms: BlockTimestampMs) -> Self {
        self.block.timestamp_ms = timestamp_ms;
        self
    }

    pub fn set_ancestors(mut self, ancestors: Vec<BlockRef>) -> Self {
        self.block.ancestors = ancestors;
        self
    }

    pub fn set_transactions(mut self, transactions: Vec<Transaction>) -> Self {
        self.block.transactions = transactions;
        self
    }

    pub fn set_commit_votes(mut self, commit_votes: Vec<CommitVote>) -> Self {
        self.block.commit_votes = commit_votes;
        self
    }

    pub fn build(self) -> Block {
        Block::V1(self.block)
    }
}

/// A block can attach reports of misbehavior by other authorities.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MisbehaviorReport {
    pub target: AuthorityIndex,
    pub proof: MisbehaviorProof,
}

/// Proof of misbehavior are usually signed block(s) from the misbehaving authority.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum MisbehaviorProof {
    InvalidBlock(BlockRef),
}

// TODO: add basic verification for BlockRef and BlockDigest.
// TODO: add tests for SignedBlock and VerifiedBlock conversion.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fastcrypto::error::FastCryptoError;

    use crate::{
        block::{SignedBlock, TestBlock},
        context::Context,
        error::ConsensusError,
    };

    #[tokio::test]
    async fn test_sign_and_verify() {
        let (context, key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);

        // Create a block that authority 2 has created
        let block = TestBlock::new(10, 2).build();

        // Create a signed block with authority's 2 private key
        let author_two_key = &key_pairs[2].1;
        let signed_block = SignedBlock::new(block, author_two_key).expect("Shouldn't fail signing");

        // Now verify the block's signature
        let result = signed_block.verify_signature(&context);
        assert!(result.is_ok());

        // Try to sign authority's 2 block with authority's 1 key
        let block = TestBlock::new(10, 2).build();
        let author_one_key = &key_pairs[1].1;
        let signed_block = SignedBlock::new(block, author_one_key).expect("Shouldn't fail signing");

        // Now verify the block, it should fail
        let result = signed_block.verify_signature(&context);
        match result.err().unwrap() {
            ConsensusError::SignatureVerificationFailure(err) => {
                assert_eq!(err, FastCryptoError::InvalidSignature);
            }
            err => panic!("Unexpected error: {err:?}"),
        }
    }
}
