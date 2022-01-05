// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::error::FastPayError;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use ed25519_dalek as dalek;
use ed25519_dalek::{Digest, PublicKey, Signer, Verifier};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;

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

pub type VersionNumber = SequenceNumber;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct UserData(pub Option<[u8; 32]>);

// TODO: Make sure secrets are not copyable and movable to control where they are in memory
pub struct KeyPair(dalek::Keypair);

#[derive(Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct PublicKeyBytes([u8; dalek::PUBLIC_KEY_LENGTH]);

impl PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn to_public_key(&self) -> Result<PublicKey, FastPayError> {
        // TODO(https://github.com/MystenLabs/fastnft/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        PublicKey::from_bytes(self.as_ref()).map_err(|_| FastPayError::InvalidAuthenticator)
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

// TODO(https://github.com/MystenLabs/fastnft/issues/101): more robust key validation
impl TryFrom<&[u8]> for PublicKeyBytes {
    type Error = FastPayError;

    fn try_from(bytes: &[u8]) -> Result<Self, FastPayError> {
        let arr: [u8; dalek::PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| FastPayError::InvalidAuthenticator)?;
        Ok(Self(arr))
    }
}

pub type PrimaryAddress = PublicKeyBytes;
pub type FastPayAddress = PublicKeyBytes;
pub type AuthorityName = PublicKeyBytes;

// Define digests and object IDs. For now, ID's are the same as Move account addresses
// (16 bytes) for easy compatibility with Move. However, we'll probably want 20+ byte
// addresses, either by changing Move to allow different address lengths or by decoupling
// addresses and ID's
pub type ObjectID = AccountAddress;
pub type ObjectRef = (ObjectID, SequenceNumber);

// TODO(https://github.com/MystenLabs/fastnft/issues/65): eventually a transaction will have a (unique) digest. For the moment we only
// have transfer transactions so we index them by the object/seq they mutate.
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct TransactionDigest((ObjectID, SequenceNumber));

// TODO: migrate TxContext type + these constants to a separate file
/// 0xB86E858F2D22236F2D96DF498FF001D0
pub const TX_CONTEXT_ADDRESS: AccountAddress = AccountAddress::new([
    0xB8, 0x6E, 0x85, 0x8F, 0x2D, 0x22, 0x23, 0x6F, 0x2D, 0x96, 0xDF, 0x49, 0x8F, 0xF0, 0x01, 0xD0,
]);
pub const TX_CONTEXT_MODULE_NAME: &IdentStr = ident_str!("TxContext");
pub const TX_CONTEXT_STRUCT_NAME: &IdentStr = TX_CONTEXT_MODULE_NAME;

#[derive(Debug)]
pub struct TxContext {
    /// Digest of the current transaction
    digest: TransactionDigest,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl TxContext {
    pub fn new(digest: TransactionDigest) -> Self {
        Self {
            digest,
            ids_created: 0,
        }
    }

    /// Derive a globally unique object ID by hashing self.digest | self.ids_created
    pub fn fresh_id(&mut self) -> ObjectID {
        // TODO(https://github.com/MystenLabs/fastnft/issues/58):
        // audit ID derivation: do we want/need domain separation, different hash function, truncation ...
        let hash_arg = &mut self.digest.0 .0.to_vec();
        hash_arg.append(&mut self.digest.0 .1 .0.to_le_bytes().to_vec());
        hash_arg.append(&mut self.ids_created.to_le_bytes().to_vec());
        let hash = Sha3_256::digest(hash_arg.as_slice());
        // truncate into an ObjectID.
        let id = AccountAddress::try_from(&hash[0..AccountAddress::LENGTH]).unwrap();

        self.ids_created += 1;

        id
    }

    // TODO(https://github.com/MystenLabs/fastnft/issues/89): temporary hack for Move compatibility
    pub fn to_bcs_bytes_hack(&self) -> Vec<u8> {
        let sender = FastPayAddress::default();
        let inputs_hash = self.digest.0 .0.to_vec();
        let obj = TxContextForMove {
            sender: sender.to_vec(),
            inputs_hash,
            ids_created: self.ids_created,
        };
        bcs::to_bytes(&obj).unwrap()
    }

    // for testing
    pub fn random() -> Self {
        Self::new(TransactionDigest::random())
    }
}

#[derive(Serialize)]
struct TxContextForMove {
    sender: Vec<u8>,
    inputs_hash: Vec<u8>,
    ids_created: u64,
}

impl TransactionDigest {
    pub fn new(id: ObjectID, seq: SequenceNumber) -> Self {
        Self((id, seq))
    }

    /// Get the mock digest of the genesis transaction
    /// TODO(https://github.com/MystenLabs/fastnft/issues/65): we can pick anything here    
    pub fn genesis() -> Self {
        Self::new(
            ObjectID::new([0u8; ObjectID::LENGTH]),
            SequenceNumber::new(),
        )
    }

    // for testing
    pub fn random() -> Self {
        Self::new(ObjectID::random(), SequenceNumber::new())
    }
}

pub fn get_key_pair() -> (FastPayAddress, KeyPair) {
    let mut csprng = OsRng;
    let keypair = dalek::Keypair::generate(&mut csprng);
    (PublicKeyBytes(keypair.public.to_bytes()), KeyPair(keypair))
}

pub fn address_as_base64<S>(key: &PublicKeyBytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&encode_address(key))
}

pub fn address_from_base64<'de, D>(deserializer: D) -> Result<PublicKeyBytes, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_address(&s).map_err(|err| serde::de::Error::custom(err.to_string()))?;
    Ok(value)
}

pub fn encode_address(key: &PublicKeyBytes) -> String {
    base64::encode(&key.0[..])
}

pub fn decode_address(s: &str) -> Result<PublicKeyBytes, anyhow::Error> {
    let value = base64::decode(s)?;
    let mut address = [0u8; dalek::PUBLIC_KEY_LENGTH];
    address.copy_from_slice(&value[..dalek::PUBLIC_KEY_LENGTH]);
    Ok(PublicKeyBytes(address))
}

pub fn dbg_addr(name: u8) -> FastPayAddress {
    let addr = [name; dalek::PUBLIC_KEY_LENGTH];
    PublicKeyBytes(addr)
}

pub fn dbg_object_id(name: u8) -> ObjectID {
    ObjectID::from_bytes([name; ObjectID::LENGTH]).unwrap()
}

#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct Signature(dalek::Signature);

impl KeyPair {
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

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64::encode(&self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for PublicKeyBytes {
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

impl std::fmt::Display for Balance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Balance {
    type Err = std::num::ParseIntError;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        Ok(Self(i128::from_str(src)?))
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

/// Something that we know how to hash and sign.
pub trait Signable<Hasher> {
    fn write(&self, hasher: &mut Hasher);
}

/// Activate the blanket implementation of `Signable` based on serde and BCS.
/// * We use `serde_name` to extract a seed from the name of structs and enums.
/// * We use `BCS` to generate canonical bytes suitable for hashing and signing.
pub trait BcsSignable: Serialize + serde::de::DeserializeOwned {}

impl<T, Hasher> Signable<Hasher> for T
where
    T: BcsSignable,
    Hasher: std::io::Write,
{
    fn write(&self, hasher: &mut Hasher) {
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        // Note: This assumes that names never contain the separator `::`.
        write!(hasher, "{}::", name).expect("Hasher should not fail");
        bcs::serialize_into(hasher, &self).expect("Message serialization should not fail");
    }
}

impl Signature {
    pub fn new<T>(value: &T, secret: &KeyPair) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        let signature = secret.0.sign(&message);
        Signature(signature)
    }

    pub fn check<T>(&self, value: &T, author: FastPayAddress) -> Result<(), FastPayError>
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        let public_key = author.to_public_key()?;
        public_key
            .verify(&message, &self.0)
            .map_err(|error| FastPayError::InvalidSignature {
                error: format!("{}", error),
            })
    }

    pub fn verify_batch<'a, T, I>(
        value: &'a T,
        votes: I,
        key_cache: &HashMap<PublicKeyBytes, PublicKey>,
    ) -> Result<(), FastPayError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = &'a (FastPayAddress, Signature)>,
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
            FastPayError::InvalidSignature {
                error: format!("{}", error),
            }
        })
    }
}
