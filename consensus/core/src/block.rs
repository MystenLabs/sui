// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use std::{
    cell::OnceCell,
    fmt,
    hash::{Hash, Hasher},
};

use fastcrypto::hash::{Digest, HashFunction};
use serde::{Deserialize, Serialize};

use consensus_config::{AuthorityIndex, DefaultHashFunction, NetworkKeySignature, DIGEST_LENGTH};

/// Round number of a block.
pub type Round = u32;

/// Block proposal timestamp in milliseconds.
pub type BlockTimestampMs = u64;

/// A block includes references to previous round blocks and transactions that the validator
/// considers valid.
/// Well behaved validators produce at most one block per round, but malicious validators can
/// equivocate.
#[derive(Clone, Deserialize, Serialize)]
#[enum_dispatch(BlockAPI)]
pub enum Block {
    V1(BlockV1),
}

impl fastcrypto::hash::Hash<{ DIGEST_LENGTH }> for Block {
    type TypedDigest = BlockDigest;

    fn digest(&self) -> BlockDigest {
        match self {
            Block::V1(block) => block.digest(),
        }
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

/// BlockRef is the minimum info that uniquely identify a block.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockRef {
    pub round: Round,
    pub author: AuthorityIndex,
    pub digest: BlockDigest,
}

impl BlockRef {
    #[cfg(test)]
    pub fn new_test(author: AuthorityIndex, round: Round, digest: BlockDigest) -> Self {
        Self {
            round,
            author,
            digest,
        }
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

impl fmt::Debug for BlockDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

/// Signature of block digest by its author.
#[allow(unused)]
pub(crate) type BlockSignature = NetworkKeySignature;

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

/// Verifiied block allows access to its content.
#[allow(unused)]
#[derive(Deserialize, Serialize)]
pub(crate) struct VerifiedBlock {
    pub block: Block,
    pub signature: bytes::Bytes,

    #[serde(skip)]
    serialized: bytes::Bytes,
}

// TODO: add basic verification for BlockRef and BlockDigest computations.
