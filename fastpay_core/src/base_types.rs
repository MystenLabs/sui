// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

use ed25519::signature::Signature as _;
use ed25519_dalek as dalek;
use ed25519_dalek::{Signer, Verifier};

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use crate::error::FastPayError;

#[cfg(test)]
#[path = "unit_tests/base_types_tests.rs"]
mod base_types_tests;

#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Default, Debug, Serialize, Deserialize,
)]
pub struct Amount(u64);
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Default, Debug, Serialize, Deserialize,
)]
pub struct Balance(i128);
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Default, Debug, Serialize, Deserialize,
)]
pub struct SequenceNumber(u64);

pub type ShardId = u32;
pub type VersionNumber = SequenceNumber;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct UserData(pub Option<[u8; 32]>);

// Ensure Secrets are not copyable and movable to control where they are in memory
// TODO: Keep the native Dalek keypair type instead of bytes.
pub struct SecretKey(pub [u8; dalek::KEYPAIR_LENGTH]);

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct EdPublicKeyBytes(pub [u8; dalek::PUBLIC_KEY_LENGTH]);

pub type PrimaryAddress = EdPublicKeyBytes;
pub type FastPayAddress = EdPublicKeyBytes;
pub type AuthorityName = EdPublicKeyBytes;

pub fn get_key_pair() -> (FastPayAddress, SecretKey) {
    let mut csprng = OsRng;
    let keypair = dalek::Keypair::generate(&mut csprng);
    (
        EdPublicKeyBytes(keypair.public.to_bytes()),
        SecretKey(keypair.to_bytes()),
    )
}

pub fn address_as_base64<S>(key: &EdPublicKeyBytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&encode_address(key))
}

pub fn address_from_base64<'de, D>(deserializer: D) -> Result<EdPublicKeyBytes, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_address(&s).map_err(|err| serde::de::Error::custom(err.to_string()))?;
    Ok(value)
}

pub fn encode_address(key: &EdPublicKeyBytes) -> String {
    base64::encode(&key.0[..])
}

pub fn decode_address(s: &str) -> Result<EdPublicKeyBytes, failure::Error> {
    let value = base64::decode(s)?;
    let mut address = [0u8; dalek::PUBLIC_KEY_LENGTH];
    address.copy_from_slice(&value[..dalek::PUBLIC_KEY_LENGTH]);
    Ok(EdPublicKeyBytes(address))
}

#[cfg(test)]
pub fn dbg_addr(name: u8) -> FastPayAddress {
    let addr = [name; dalek::PUBLIC_KEY_LENGTH];
    EdPublicKeyBytes(addr)
}

// TODO: Remove Eq, PartialEq, Ord, PartialOrd and Hash from signatures.
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct Signature {
    pub part1: [u8; dalek::SIGNATURE_LENGTH / 2],
    pub part2: [u8; dalek::SIGNATURE_LENGTH / 2],
}

// Zero the secret key when unallocating.
impl Drop for SecretKey {
    fn drop(&mut self) {
        for i in 0..dalek::KEYPAIR_LENGTH {
            self.0[i] = 0;
        }
    }
}

impl SecretKey {
    pub fn copy(&self) -> SecretKey {
        let mut sec_bytes = SecretKey {
            0: [0; dalek::KEYPAIR_LENGTH],
        };
        sec_bytes.0.copy_from_slice(&(self.0)[..]);
        sec_bytes
    }
}

impl Serialize for SecretKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&base64::encode(&self.0[..]))
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D>(deserializer: D) -> Result<SecretKey, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = base64::decode(&s).map_err(|err| serde::de::Error::custom(err.to_string()))?;
        let mut key = [0u8; dalek::KEYPAIR_LENGTH];
        key.copy_from_slice(&value[..dalek::KEYPAIR_LENGTH]);
        Ok(SecretKey(key))
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64::encode(&self.to_array()[..]);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for EdPublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64::encode(&self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl Amount {
    pub fn zero() -> Self {
        Amount(0)
    }

    pub fn try_add(self, other: Self) -> Result<Self, FastPayError> {
        let val = self.0.checked_add(other.0);
        match val {
            None => Err(FastPayError::AmountOverflow),
            Some(val) => Ok(Self(val)),
        }
    }

    pub fn try_sub(self, other: Self) -> Result<Self, FastPayError> {
        let val = self.0.checked_sub(other.0);
        match val {
            None => Err(FastPayError::AmountUnderflow),
            Some(val) => Ok(Self(val)),
        }
    }
}

impl Balance {
    pub fn zero() -> Self {
        Balance(0)
    }

    pub fn max() -> Self {
        Balance(std::i128::MAX)
    }

    pub fn try_add(&self, other: Self) -> Result<Self, FastPayError> {
        let val = self.0.checked_add(other.0);
        match val {
            None => Err(FastPayError::BalanceOverflow),
            Some(val) => Ok(Self(val)),
        }
    }

    pub fn try_sub(&self, other: Self) -> Result<Self, FastPayError> {
        let val = self.0.checked_sub(other.0);
        match val {
            None => Err(FastPayError::BalanceUnderflow),
            Some(val) => Ok(Self(val)),
        }
    }
}

impl From<Amount> for u64 {
    fn from(val: Amount) -> Self {
        val.0
    }
}

impl From<Amount> for Balance {
    fn from(val: Amount) -> Self {
        Balance(val.0 as i128)
    }
}

impl TryFrom<Balance> for Amount {
    type Error = std::num::TryFromIntError;

    fn try_from(val: Balance) -> Result<Self, Self::Error> {
        Ok(Amount(val.0.try_into()?))
    }
}

impl SequenceNumber {
    pub fn new() -> Self {
        SequenceNumber(0)
    }

    pub fn max() -> Self {
        SequenceNumber(0x7fff_ffff_ffff_ffff)
    }

    pub fn increment(self) -> Result<SequenceNumber, FastPayError> {
        let val = self.0.checked_add(1);
        match val {
            None => Err(FastPayError::SequenceOverflow),
            Some(val) => Ok(Self(val)),
        }
    }

    pub fn decrement(self) -> Result<SequenceNumber, FastPayError> {
        let val = self.0.checked_sub(1);
        match val {
            None => Err(FastPayError::SequenceUnderflow),
            Some(val) => Ok(Self(val)),
        }
    }
}

impl From<SequenceNumber> for u64 {
    fn from(val: SequenceNumber) -> Self {
        val.0
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Amount(value)
    }
}

impl From<i128> for Balance {
    fn from(value: i128) -> Self {
        Balance(value)
    }
}

impl From<u64> for SequenceNumber {
    fn from(value: u64) -> Self {
        SequenceNumber(value)
    }
}

impl From<SequenceNumber> for usize {
    fn from(value: SequenceNumber) -> Self {
        value.0 as usize
    }
}

pub trait Digestible {
    fn digest(&self) -> [u8; 32];
}

#[cfg(test)]
impl Digestible for [u8; 5] {
    fn digest(self: &[u8; 5]) -> [u8; 32] {
        use ed25519_dalek::Digest;

        let mut h = dalek::Sha512::new();
        let mut hash = [0u8; 64];
        let mut digest = [0u8; 32];
        h.update(&self);
        hash.copy_from_slice(h.finalize().as_slice());
        digest.copy_from_slice(&hash[..32]);
        digest
    }
}

impl Signature {
    pub fn to_array(&self) -> [u8; 64] {
        let mut sig: [u8; 64] = [0; 64];
        sig[0..32].clone_from_slice(&self.part1);
        sig[32..64].clone_from_slice(&self.part2);
        sig
    }

    pub fn new<T>(value: &T, secret: &SecretKey) -> Self
    where
        T: Digestible,
    {
        let message = value.digest();
        let key_pair = dalek::Keypair::from_bytes(&secret.0).unwrap();
        let signature = key_pair.sign(&message);
        let sig_bytes = signature.to_bytes();

        let mut part1 = [0; 32];
        let mut part2 = [0; 32];
        part1.clone_from_slice(&sig_bytes[0..32]);
        part2.clone_from_slice(&sig_bytes[32..64]);

        Signature { part1, part2 }
    }

    fn check_internal<T>(
        &self,
        value: &T,
        author: FastPayAddress,
    ) -> Result<(), dalek::SignatureError>
    where
        T: Digestible,
    {
        let message = value.digest();
        let public_key = dalek::PublicKey::from_bytes(&author.0)?;
        let sig = self.to_array();
        let dalex_sig = dalek::Signature::from_bytes(&sig)?;
        public_key.verify(&message, &dalex_sig)
    }

    pub fn check<T>(&self, value: &T, author: FastPayAddress) -> Result<(), FastPayError>
    where
        T: Digestible,
    {
        self.check_internal(value, author)
            .map_err(|error| FastPayError::InvalidSignature {
                error: format!("{}", error),
            })
    }

    fn verify_batch_internal<'a, T, I>(value: &'a T, votes: I) -> Result<(), dalek::SignatureError>
    where
        T: Digestible,
        I: IntoIterator<Item = &'a (FastPayAddress, Signature)>,
    {
        let msg: &[u8] = &value.digest();
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<dalek::Signature> = Vec::new();
        let mut public_keys: Vec<dalek::PublicKey> = Vec::new();
        for (addr, sig) in votes.into_iter() {
            messages.push(msg);
            signatures.push(dalek::Signature::from_bytes(&sig.to_array())?);
            public_keys.push(dalek::PublicKey::from_bytes(&addr.0)?);
        }
        dalek::verify_batch(&messages[..], &signatures[..], &public_keys[..])
    }

    pub fn verify_batch<'a, T, I>(value: &'a T, votes: I) -> Result<(), FastPayError>
    where
        T: Digestible,
        I: IntoIterator<Item = &'a (FastPayAddress, Signature)>,
    {
        Signature::verify_batch_internal(value, votes).map_err(|error| {
            FastPayError::InvalidSignature {
                error: format!("{}", error),
            }
        })
    }
}
