// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use crate::sui_serde::Readable;
use fastcrypto::encoding::{Base58, Encoding};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};

/// A representation of a SHA3-256 Digest
#[serde_as]
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Sha3Digest(
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, Bytes>")]
    [u8; 32],
);

impl Sha3Digest {
    pub fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(mut rng: R) -> Self {
        let mut bytes = [0; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn random() -> Self {
        Self::generate(rand::thread_rng())
    }

    pub fn inner(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0
    }
}

impl AsRef<[u8]> for Sha3Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8; 32]> for Sha3Digest {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<Sha3Digest> for [u8; 32] {
    fn from(digest: Sha3Digest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for Sha3Digest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for Sha3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO avoid the allocation
        f.write_str(&Base58::encode(self.0))
    }
}

impl fmt::Debug for Sha3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Representation of a Checkpoint's digest
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct CheckpointDigest(Sha3Digest);

impl CheckpointDigest {
    pub fn new(digest: [u8; 32]) -> Self {
        Self(Sha3Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Sha3Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Sha3Digest::random())
    }

    pub fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }
}

impl AsRef<[u8]> for CheckpointDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for CheckpointDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl From<CheckpointDigest> for [u8; 32] {
    fn from(digest: CheckpointDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for CheckpointDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CheckpointDigest").field(&self.0).finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointContentsDigest(Sha3Digest);

impl CheckpointContentsDigest {
    pub fn new(digest: [u8; 32]) -> Self {
        Self(Sha3Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Sha3Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Sha3Digest::random())
    }

    pub fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }
}

impl AsRef<[u8]> for CheckpointContentsDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for CheckpointContentsDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl From<CheckpointContentsDigest> for [u8; 32] {
    fn from(digest: CheckpointContentsDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for CheckpointContentsDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CheckpointContentsDigest")
            .field(&self.0)
            .finish()
    }
}
