// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use async_graphql::*;
use fastcrypto::encoding::{Base58, Encoding};
use std::fmt;

pub(crate) const BASE58_DIGEST_LENGTH: usize = 32;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
pub(crate) struct Digest([u8; BASE58_DIGEST_LENGTH]);

impl Digest {
    pub fn into_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn from_array(arr: [u8; BASE58_DIGEST_LENGTH]) -> Self {
        Digest(arr)
    }
}

impl TryFrom<Vec<u8>> for Digest {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; BASE58_DIGEST_LENGTH] = <[u8; BASE58_DIGEST_LENGTH]>::try_from(&bytes[..])
            .map_err(|_| Error::InvalidDigestLength {
                expected: BASE58_DIGEST_LENGTH,
                actual: bytes.len(),
            })?;

        Ok(Self::from_array(bytes))
    }
}

impl TryFrom<&[u8]> for Digest {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; BASE58_DIGEST_LENGTH] = <[u8; BASE58_DIGEST_LENGTH]>::try_from(bytes)
            .map_err(|_| Error::InvalidDigestLength {
                expected: BASE58_DIGEST_LENGTH,
                actual: bytes.len(),
            })?;

        Ok(Self::from_array(bytes))
    }
}

impl std::str::FromStr for Digest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0u8; BASE58_DIGEST_LENGTH];
        result
            .copy_from_slice(&Base58::decode(s).map_err(|r| Error::InvalidBase58(format!("{r}")))?);
        Ok(Digest(result))
    }
}

impl std::string::ToString for Digest {
    fn to_string(&self) -> String {
        Base58::encode(self.0)
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Digest")
            .field(&Base58::encode(self.0))
            .finish()
    }
}

impl fmt::LowerHex for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02x}", byte)?;
        }

        Ok(())
    }
}

impl fmt::UpperHex for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02X}", byte)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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
        assert!(Digest::from_str("ILoveBase58").is_err());
    }
}
