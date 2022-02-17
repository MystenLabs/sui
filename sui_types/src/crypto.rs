// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use std::collections::HashMap;

use ed25519_dalek as dalek;
use ed25519_dalek::{Digest, PublicKey, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sha3::Sha3_256;

use crate::error::SuiError;

// TODO: Make sure secrets are not copyable and movable to control where they are in memory
#[derive(Debug)]
pub struct KeyPair(dalek::Keypair);

impl KeyPair {
    pub fn public(&self) -> PublicKeyBytes {
        PublicKeyBytes(self.0.public.to_bytes())
    }

    /// Avoid implementing `clone` on secret keys to prevent mistakes.
    #[must_use]
    pub fn copy(&self) -> KeyPair {
        KeyPair(dalek::Keypair {
            secret: dalek::SecretKey::from_bytes(self.0.secret.as_bytes()).unwrap(),
            public: dalek::PublicKey::from_bytes(self.0.public.as_bytes()).unwrap(),
        })
    }
}

impl Serialize for KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&base64::encode(&self.0.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<KeyPair, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = base64::decode(&s).map_err(|err| serde::de::Error::custom(err.to_string()))?;
        let key = dalek::Keypair::from_bytes(&value)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        Ok(KeyPair(key))
    }
}

impl signature::Signer<ed25519_dalek::Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<ed25519_dalek::Signature, signature::Error> {
        self.0.try_sign(msg)
    }
}

#[serde_as]
#[derive(Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct PublicKeyBytes(#[serde_as(as = "Bytes")] [u8; dalek::PUBLIC_KEY_LENGTH]);

impl PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn to_public_key(&self) -> Result<PublicKey, SuiError> {
        // TODO(https://github.com/MystenLabs/fastnft/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        PublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

// TODO(https://github.com/MystenLabs/fastnft/issues/101): more robust key validation
impl TryFrom<&[u8]> for PublicKeyBytes {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; dalek::PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidAuthenticator)?;
        Ok(Self(arr))
    }
}

impl From<Vec<u8>> for PublicKeyBytes {
    fn from(bytes: Vec<u8>) -> Self {
        let mut result = [0u8; dalek::PUBLIC_KEY_LENGTH];
        result.copy_from_slice(&bytes[..dalek::PUBLIC_KEY_LENGTH]);
        Self(result)
    }
}

impl std::fmt::Debug for PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

// TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
pub fn get_key_pair() -> (PublicKeyBytes, KeyPair) {
    let mut csprng = OsRng;
    let keypair = dalek::Keypair::generate(&mut csprng);
    (PublicKeyBytes(keypair.public.to_bytes()), KeyPair(keypair))
}

pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (PublicKeyBytes, KeyPair) {
    let keypair = dalek::Keypair::from_bytes(bytes).unwrap();
    (PublicKeyBytes(keypair.public.to_bytes()), KeyPair(keypair))
}
#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct Signature(dalek::Signature);

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64::encode(&self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl Signature {
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<ed25519_dalek::Signature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        let signature = secret.sign(&message);
        Signature(signature)
    }

    pub fn check<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        let public_key = author.to_public_key()?;
        public_key
            .verify(&message, &self.0)
            .map_err(|error| SuiError::InvalidSignature {
                error: format!("{}", error),
            })
    }

    pub fn verify_batch<'a, T, I>(
        value: &'a T,
        votes: I,
        key_cache: &HashMap<PublicKeyBytes, PublicKey>,
    ) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = &'a (PublicKeyBytes, Signature)>,
    {
        let mut msg = Vec::new();
        value.write(&mut msg);
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<dalek::Signature> = Vec::new();
        let mut public_keys: Vec<dalek::PublicKey> = Vec::new();
        for (addr, sig) in votes.into_iter() {
            messages.push(&msg);
            signatures.push(sig.0);
            match key_cache.get(addr) {
                Some(v) => public_keys.push(*v),
                None => public_keys.push(addr.to_public_key()?),
            }
        }
        dalek::verify_batch(&messages[..], &signatures[..], &public_keys[..]).map_err(|error| {
            SuiError::InvalidSignature {
                error: format!("{}", error),
            }
        })
    }
}

/// Something that we know how to hash and sign.
pub trait Signable<W> {
    fn write(&self, writer: &mut W);
}

/// Activate the blanket implementation of `Signable` based on serde and BCS.
/// * We use `serde_name` to extract a seed from the name of structs and enums.
/// * We use `BCS` to generate canonical bytes suitable for hashing and signing.
pub trait BcsSignable: Serialize + serde::de::DeserializeOwned {}

impl<T, W> Signable<W> for T
where
    T: BcsSignable,
    W: std::io::Write,
{
    fn write(&self, writer: &mut W) {
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        // Note: This assumes that names never contain the separator `::`.
        write!(writer, "{}::", name).expect("Hasher should not fail");
        bcs::serialize_into(writer, &self).expect("Message serialization should not fail");
    }
}

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}
