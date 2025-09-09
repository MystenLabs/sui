// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use fastcrypto::encoding::{Base58, Encoding};
use sui_types::digests::TransactionDigest;

use super::impl_string_input;

const DIGEST_LENGTH: usize = 32;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Digest([u8; DIGEST_LENGTH]);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid Base58: {0}")]
    InvalidBase58(String),

    #[error("Expected digest to be {}B, but got {0}B", DIGEST_LENGTH)]
    BadDigestLength(usize),
}

impl_string_input!(Digest);

impl From<Digest> for TransactionDigest {
    fn from(digest: Digest) -> Self {
        digest.0.into()
    }
}

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
        let mut result = [0u8; DIGEST_LENGTH];

        if value.len() != DIGEST_LENGTH {
            return Err(Error::BadDigestLength(value.len()));
        }

        result.copy_from_slice(value);
        Ok(Digest(result))
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::*;

    #[test]
    fn test_parse_valid_digest() {
        assert_eq!(
            Digest::from_str("DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU").unwrap(),
            Digest([
                183, 119, 223, 39, 204, 68, 220, 4, 126, 234, 232, 146, 106, 249, 98, 12, 170, 209,
                98, 203, 243, 77, 154, 225, 177, 216, 169, 101, 51, 116, 79, 223,
            ])
        );
    }

    #[test]
    fn test_parse_invalid_digest() {
        assert!(matches!(
            Digest::from_str("ILoveBase58").unwrap_err(),
            Error::InvalidBase58(_),
        ));
    }

    #[test]
    fn test_parse_long_digest() {
        assert!(matches!(
            Digest::from_str(&Base58::encode(vec![0u8; DIGEST_LENGTH + 1])).unwrap_err(),
            Error::BadDigestLength(_),
        ))
    }

    #[test]
    fn test_parse_short_digest() {
        assert!(matches!(
            Digest::from_str(&Base58::encode(vec![0u8; DIGEST_LENGTH - 1])).unwrap_err(),
            Error::BadDigestLength(_),
        ))
    }
}
