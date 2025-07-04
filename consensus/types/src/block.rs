// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt,
    hash::{Hash, Hasher},
};

use consensus_config::{AuthorityIndex, DIGEST_LENGTH};

use fastcrypto::hash::Digest;
use serde::{Deserialize, Serialize};

/// Round number of a block.
pub type Round = u32;

/// Block proposal timestamp in milliseconds.
pub type BlockTimestampMs = u64;

/// Index of a transaction in a block.
pub type TransactionIndex = u16;

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
pub struct BlockDigest(pub [u8; consensus_config::DIGEST_LENGTH]);

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
