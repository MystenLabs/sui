// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};

use bytes::Buf as _;
use consensus_config::{AuthorityIndex, DefaultHashFunction, DIGEST_LENGTH};
use enum_dispatch::enum_dispatch;
use fastcrypto::hash::{Digest, HashFunction};
use serde::{Deserialize, Serialize};

/// Block proposal timestamp in milliseconds.
pub type BlockTimestampMs = u64;

/// Round number of a block.
pub type Round = u32;

/// A block includes references to previous round blocks and transactions that the validator
/// considers valid.
/// Well behaved validators produce at most one block per round, but malicious validators can
/// equivocate.
#[derive(Clone, Deserialize, Serialize)]
#[enum_dispatch(BlockAPI)]
pub enum Block {
    V1(BlockV1),
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

impl Eq for Block {}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Block {:?} {{", self.reference())?;
        write!(
            f,
            "ancestors({})={:?},",
            self.ancestors().len(),
            self.ancestors()
        )?;
        // TODO: add printing of transactions.
        writeln!(f, "}}")
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reference())
    }
}

#[enum_dispatch]
pub trait BlockAPI {
    fn reference(&self) -> BlockRef;
    fn digest(&self) -> BlockDigest;
    fn round(&self) -> Round;
    fn author(&self) -> AuthorityIndex;
    fn timestamp_ms(&self) -> BlockTimestampMs;
    fn ancestors(&self) -> &[BlockRef];
    // TODO: add accessor for transactions.
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct BlockV1 {
    round: Round,
    author: AuthorityIndex,
    timestamp_ms: BlockTimestampMs,
    ancestors: Vec<BlockRef>,

    #[serde(skip)]
    digest: OnceCell<BlockDigest>,
}

impl BlockAPI for BlockV1 {
    fn reference(&self) -> BlockRef {
        BlockRef {
            round: self.round,
            author: self.author,
            digest: self.digest(),
        }
    }

    fn digest(&self) -> BlockDigest {
        *self.digest.get_or_init(|| {
            let mut hasher = DefaultHashFunction::new();
            hasher.update(bcs::to_bytes(&self).expect("Serialization should not fail"));
            BlockDigest(hasher.finalize().into())
        })
    }

    fn round(&self) -> Round {
        self.round
    }

    fn author(&self) -> AuthorityIndex {
        self.author
    }

    fn timestamp_ms(&self) -> BlockTimestampMs {
        self.timestamp_ms
    }

    fn ancestors(&self) -> &[BlockRef] {
        &self.ancestors
    }
}

impl PartialEq for BlockV1 {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

/// BlockRef is the minimum info that uniquely identify a block.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockRef {
    pub round: Round,
    pub author: AuthorityIndex,
    pub digest: BlockDigest,
}

#[allow(unused)]
impl BlockRef {
    pub fn new(round: Round, author: AuthorityIndex, digest: BlockDigest) -> Self {
        Self {
            round,
            author,
            digest,
        }
    }
}

impl fmt::Display for BlockRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}({};{})", self.round, self.author, self.digest)
    }
}

impl fmt::Debug for BlockRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}({};{:?})", self.round, self.author, self.digest)
    }
}

impl Hash for BlockRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.digest.0[..8]);
    }
}

/// Hash of a block, covers all fields except signature.
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

#[derive(Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default, Hash)]
pub(crate) struct Slot {
    pub round: Round,
    pub authority: AuthorityIndex,
}

#[allow(unused)]
impl Slot {
    pub fn new(authority: AuthorityIndex, round: Round) -> Self {
        Self { authority, round }
    }
}

impl From<BlockRef> for Slot {
    fn from(value: BlockRef) -> Self {
        Slot::new(value.author, value.round)
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.round, self.authority)
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// Unverified block only allows limited access to its content.
#[allow(unused)]
#[derive(Deserialize)]
pub(crate) struct SignedBlock {
    block: Block,
    signature: bytes::Bytes,

    #[serde(skip)]
    serialized: bytes::Bytes,
}

impl SignedBlock {
    // TODO: add deserialization and verification.
}

/// Verifiied block allows full access to its content.
#[allow(unused)]
#[derive(Clone, Deserialize, PartialEq)]
pub(crate) struct VerifiedBlock {
    block: Block,
    signature: bytes::Bytes,

    #[serde(skip)]
    serialized: bytes::Bytes,
}

impl VerifiedBlock {
    /// Parses a serialized block from storage, where the block has been verified.
    /// This should never be called on unverified data received over the network.
    pub fn parse_from_storage(serialized: bytes::Bytes) -> Result<Self, bcs::Error> {
        let mut block: VerifiedBlock = bcs::from_bytes(serialized.chunk())?;
        block.serialized = serialized;
        Ok(block)
    }

    pub fn serialized(&self) -> &bytes::Bytes {
        &self.serialized
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(block: Block) -> Self {
        let serialized: bytes::Bytes = bcs::to_bytes(&(&block, bytes::Bytes::default()))
            .expect("Serialization should not fail")
            .into();
        VerifiedBlock {
            block,
            signature: Default::default(),
            serialized,
        }
    }
}

impl Deref for VerifiedBlock {
    type Target = Block;

    fn deref(&self) -> &Self::Target {
        &self.block
    }
}

impl fmt::Display for VerifiedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.block.reference())
    }
}

impl fmt::Debug for VerifiedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{:?}({};{:?};v)",
            self.reference(),
            self.timestamp_ms(),
            self.ancestors()
        )
    }
}

/// Creates fake blocks for testing.
#[cfg(test)]
pub(crate) struct TestBlock {
    block: BlockV1,
}

#[allow(unused)]
#[cfg(test)]
impl TestBlock {
    pub(crate) fn new(round: Round, author: u32) -> Self {
        Self {
            block: BlockV1 {
                round,
                author: AuthorityIndex::new_for_test(author),
                ..Default::default()
            },
        }
    }

    pub(crate) fn set_round(mut self, round: Round) -> Self {
        self.block.round = round;
        self
    }

    pub(crate) fn set_author(mut self, author: AuthorityIndex) -> Self {
        self.block.author = author;
        self
    }

    pub(crate) fn set_timestamp_ms(mut self, timestamp_ms: BlockTimestampMs) -> Self {
        self.block.timestamp_ms = timestamp_ms;
        self
    }

    pub(crate) fn set_ancestors(mut self, ancestors: Vec<BlockRef>) -> Self {
        self.block.ancestors = ancestors;
        self
    }

    pub(crate) fn build(self) -> Block {
        Block::V1(self.block)
    }
}

// TODO: add basic verification for BlockRef and BlockDigest computations.
