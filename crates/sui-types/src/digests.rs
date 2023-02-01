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
