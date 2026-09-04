// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Here we select the cryptographic types that are used by default in the code base.
//! The whole code base should only:
//! - refer to those aliases and not use the individual scheme implementations
//! - not use the schemes in a way that break genericity (e.g. using their Struct impl functions)
//! - swap one of those aliases to point to another type if necessary
//!
//! Beware: if you change those aliases to point to another scheme implementation, you will have
//! to change all four aliases to point to concrete types that work with each other. Failure to do
//! so will result in a ton of compilation errors, and worse: it will not make sense!

use fastcrypto::{
    ed25519,
    encoding::{Base64, Encoding as _},
    error::FastCryptoError,
    hash::{Blake2b256, HashFunction},
    traits::{KeyPair as _, Signer as _, ToFromBytes as _, VerifyingKey as _},
};
use serde::{Deserialize, Serialize};
use shared_crypto::intent::INTENT_PREFIX_LENGTH;

/// Network key is used for TLS and as the network identity of the authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NetworkPublicKey(ed25519::Ed25519PublicKey);
pub struct NetworkPrivateKey(ed25519::Ed25519PrivateKey);
pub struct NetworkKeyPair(ed25519::Ed25519KeyPair);

impl NetworkPublicKey {
    pub fn new(key: ed25519::Ed25519PublicKey) -> Self {
        Self(key)
    }

    pub fn into_inner(self) -> ed25519::Ed25519PublicKey {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.0.to_bytes()
    }
}

impl NetworkPrivateKey {
    pub fn into_inner(self) -> ed25519::Ed25519PrivateKey {
        self.0
    }
}

impl NetworkKeyPair {
    pub fn new(keypair: ed25519::Ed25519KeyPair) -> Self {
        Self(keypair)
    }

    pub fn generate<R: rand::Rng + fastcrypto::traits::AllowedRng>(rng: &mut R) -> Self {
        Self(ed25519::Ed25519KeyPair::generate(rng))
    }

    pub fn public(&self) -> NetworkPublicKey {
        NetworkPublicKey(self.0.public().clone())
    }

    pub fn private_key(self) -> NetworkPrivateKey {
        NetworkPrivateKey(self.0.copy().private())
    }

    pub fn private_key_bytes(self) -> [u8; 32] {
        self.0.private().0.to_bytes()
    }
}

impl Clone for NetworkKeyPair {
    fn clone(&self) -> Self {
        Self(self.0.copy())
    }
}

/// Protocol key is used for signing blocks and verifying block signatures.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtocolPublicKey(ed25519::Ed25519PublicKey);
pub struct ProtocolKeyPair(ed25519::Ed25519KeyPair);
pub struct ProtocolKeySignature(ed25519::Ed25519Signature);

impl ProtocolPublicKey {
    pub fn new(key: ed25519::Ed25519PublicKey) -> Self {
        Self(key)
    }

    pub fn verify(
        &self,
        message: &[u8],
        signature: &ProtocolKeySignature,
    ) -> Result<(), FastCryptoError> {
        self.0.verify(message, &signature.0)
    }

    pub fn to_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl ProtocolKeyPair {
    pub fn new(keypair: ed25519::Ed25519KeyPair) -> Self {
        Self(keypair)
    }

    pub fn generate<R: rand::Rng + fastcrypto::traits::AllowedRng>(rng: &mut R) -> Self {
        Self(ed25519::Ed25519KeyPair::generate(rng))
    }

    pub fn public(&self) -> ProtocolPublicKey {
        ProtocolPublicKey(self.0.public().clone())
    }

    pub fn sign(&self, message: &[u8]) -> ProtocolKeySignature {
        ProtocolKeySignature(self.0.sign(message))
    }
}

impl Clone for ProtocolKeyPair {
    fn clone(&self) -> Self {
        Self(self.0.copy())
    }
}

impl ProtocolKeySignature {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        Ok(Self(ed25519::Ed25519Signature::from_bytes(bytes)?))
    }

    pub fn to_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Authority name is a raw bytes identity for an authority, matching `AuthorityName`
/// on the Sui side. It is only used for identity sanity checks and not for cryptographic
/// verification, so its length is not tied to any particular signature scheme: the bytes are
/// whatever the Sui side derives the authority's identity from (a BLS12381 public key today).
/// Any expectation about the length belongs to the code constructing the committee.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorityName(Vec<u8>);

impl AuthorityName {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for AuthorityName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AuthorityName({})", Base64::encode(&self.0))
    }
}

impl Serialize for AuthorityName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&Base64::encode(&self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for AuthorityName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Base64::decode(&s).map_err(serde::de::Error::custom)?
        } else {
            <Vec<u8>>::deserialize(deserializer)?
        };
        Ok(Self(bytes))
    }
}

/// Defines algorithm and format of block and commit digests.
pub type DefaultHashFunction = Blake2b256;
pub const DIGEST_LENGTH: usize = DefaultHashFunction::OUTPUT_SIZE;
pub const INTENT_MESSAGE_LENGTH: usize = INTENT_PREFIX_LENGTH + DIGEST_LENGTH;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_name_roundtrips_at_any_length() {
        for len in [0usize, 1, 32, 48, 96, 1024] {
            let name = AuthorityName::new(vec![len as u8; len]);
            assert_eq!(name.len(), len);
            assert_eq!(name.is_empty(), len == 0);

            let binary = bcs::to_bytes(&name).unwrap();
            assert_eq!(bcs::from_bytes::<AuthorityName>(&binary).unwrap(), name);

            let json = serde_json::to_string(&name).unwrap();
            assert_eq!(serde_json::from_str::<AuthorityName>(&json).unwrap(), name);
        }
    }

    /// The 96 byte BLS12381 encoding must stay byte-identical to what the previous fixed-length
    /// `AuthorityName` produced, so committees serialized by either version interoperate.
    #[test]
    fn authority_name_binary_encoding_is_length_prefixed_bytes() {
        let name = AuthorityName::new([7u8; 96]);
        let mut expected = vec![96u8];
        expected.extend_from_slice(&[7u8; 96]);
        assert_eq!(bcs::to_bytes(&name).unwrap(), expected);
    }

    /// Committee ordering relies on `Ord`, which must stay lexicographic across mixed lengths.
    #[test]
    fn authority_name_orders_lexicographically() {
        let mut names = vec![
            AuthorityName::new(vec![2u8]),
            AuthorityName::new(vec![1u8, 1u8]),
            AuthorityName::new(vec![1u8]),
        ];
        names.sort();
        assert_eq!(
            names,
            vec![
                AuthorityName::new(vec![1u8]),
                AuthorityName::new(vec![1u8, 1u8]),
                AuthorityName::new(vec![2u8]),
            ]
        );
    }
}
