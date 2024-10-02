// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::string_input::impl_string_input;
use async_graphql::*;
use fastcrypto::encoding::{Base58, Encoding};
use std::{fmt, str::FromStr};
use sui_types::digests::{ObjectDigest, TransactionDigest};

pub(crate) const BASE58_DIGEST_LENGTH: usize = 32;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Digest([u8; BASE58_DIGEST_LENGTH]);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid Base58: {0}")]
    InvalidBase58(String),

    #[error("Expected digest to be {expect}B, but got {actual}B")]
    BadDigestLength { expect: usize, actual: usize },
}

impl Digest {
    pub(crate) fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl_string_input!(Digest);

impl FromStr for Digest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let buffer = Base58::decode(s).map_err(|_| Error::InvalidBase58(s.to_string()))?;
        Digest::try_from(&buffer[..])
    }
}

impl TryFrom<&[u8]> for Digest {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Error> {
        let mut result = [0u8; BASE58_DIGEST_LENGTH];

        if value.len() != BASE58_DIGEST_LENGTH {
            return Err(Error::BadDigestLength {
                expect: BASE58_DIGEST_LENGTH,
                actual: value.len(),
            });
        }

        result.copy_from_slice(value);
        Ok(Digest(result))
    }
}

impl From<Digest> for ObjectDigest {
    fn from(digest: Digest) -> Self {
        ObjectDigest::new(digest.0)
    }
}

impl From<TransactionDigest> for Digest {
    fn from(digest: TransactionDigest) -> Self {
        Digest(digest.into_inner())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Base58::encode(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::*;

    #[test]
    fn test_base58_digest() {
        let digest = [
            183u8, 119, 223, 39, 204, 68, 220, 4, 126, 234, 232, 146, 106, 249, 98, 12, 170, 209,
            98, 203, 243, 77, 154, 225, 177, 216, 169, 101, 51, 116, 79, 223,
        ];

        assert_eq!(
            Digest::from_str("DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU").unwrap(),
            Digest(digest)
        );

        assert!(matches!(
            Digest::from_str("ILoveBase58").unwrap_err(),
            Error::InvalidBase58(_),
        ));

        let long_digest = {
            let mut bytes = vec![];
            bytes.extend(digest);
            bytes.extend(digest);
            Base58::encode(bytes)
        };

        assert!(matches!(
            Digest::from_str(&long_digest).unwrap_err(),
            Error::BadDigestLength { .. },
        ))
    }
}
