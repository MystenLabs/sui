// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::sealed::SealedPublicKeyLength;
use crate::traits::{ToFromBytes, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use std::{fmt::Display, marker::PhantomData, str::FromStr};

/// A generic construction representing bytes who claim to be the instance of a public key.
#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PublicKeyBytes<T, const N: usize> {
    #[serde_as(as = "Bytes")]
    bytes: [u8; N],
    phantom: PhantomData<T>,
}

impl<T, const N: usize> AsRef<[u8]> for PublicKeyBytes<T, N>
where
    T: VerifyingKey,
{
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..]
    }
}

impl<T, const N: usize> Display for PublicKeyBytes<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.bytes);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl<T: VerifyingKey, const N: usize> ToFromBytes for PublicKeyBytes<T, N> {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let bytes: [u8; N] = bytes.try_into().map_err(signature::Error::from_source)?;
        Ok(PublicKeyBytes {
            bytes,
            phantom: PhantomData,
        })
    }
}

impl<T, const N: usize> PublicKeyBytes<T, N> {
    /// This ensures it's impossible to construct an instance with other than registered lengths
    pub fn new(bytes: [u8; N]) -> PublicKeyBytes<T, N>
    where
        PublicKeyBytes<T, N>: SealedPublicKeyLength,
    {
        PublicKeyBytes {
            bytes,
            phantom: PhantomData,
        }
    }
}

impl<T, const N: usize> Default for PublicKeyBytes<T, N> {
    // this is probably derivable, but we'd rather have it explicitly laid out for instructional purposes,
    // see [#34](https://github.com/MystenLabs/narwhal/issues/34)
    fn default() -> Self {
        Self {
            bytes: [0u8; N],
            phantom: PhantomData,
        }
    }
}

impl<T: VerifyingKey, const N: usize> FromStr for PublicKeyBytes<T, N> {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let value = hex::decode(s)?;
        Self::from_bytes(&value[..]).map_err(|_| anyhow::anyhow!("byte deserialization failed"))
    }
}

impl<T: VerifyingKey, const N: usize> Copy for PublicKeyBytes<T, N> {}

// This guarantees the security of the constructor of a `PublicKeyBytes` instance
// TODO: replace this clunky sealed marker trait once feature(associated_const_equality) stabilizes
mod sealed {
    #[cfg(feature = "celo")]
    use crate::bls12377::BLS12377PublicKeyBytes;

    use crate::{
        bls12381::BLS12381PublicKeyBytes, ed25519::Ed25519PublicKeyBytes,
        secp256k1::Secp256k1PublicKeyBytes,
    };

    pub trait SealedPublicKeyLength {}
    impl SealedPublicKeyLength for Ed25519PublicKeyBytes {}
    impl SealedPublicKeyLength for BLS12381PublicKeyBytes {}
    impl SealedPublicKeyLength for Secp256k1PublicKeyBytes {}
    #[cfg(feature = "celo")]
    impl SealedPublicKeyLength for BLS12377PublicKeyBytes {}
}
