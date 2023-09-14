// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use fastcrypto::encoding::{Base58, Encoding};
use std::fmt;

const BASE58_DIGEST_LENGTH: usize = 32;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
pub(crate) struct Digest([u8; BASE58_DIGEST_LENGTH]);

impl Digest {
    pub fn into_array(self) -> [u8; BASE58_DIGEST_LENGTH] {
        self.0
    }

    pub fn from_array(arr: [u8; BASE58_DIGEST_LENGTH]) -> Self {
        Digest(arr)
    }
}

impl std::str::FromStr for Digest {
    type Err = InputValueError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0u8; BASE58_DIGEST_LENGTH];
        result.copy_from_slice(&Base58::decode(s).map_err(InputValueError::custom)?);
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
