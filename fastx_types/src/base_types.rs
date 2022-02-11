// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::error::SuiError;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use ed25519_dalek as dalek;
use ed25519_dalek::{Digest, PublicKey, Verifier};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sha3::Sha3_256;

#[cfg(test)]
#[path = "unit_tests/base_types_tests.rs"]
mod base_types_tests;

#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Default, Debug, Serialize, Deserialize,
)]
pub struct SequenceNumber(u64);

pub type VersionNumber = SequenceNumber;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct UserData(pub Option<[u8; 32]>);

// TODO: Make sure secrets are not copyable and movable to control where they are in memory
#[derive(Debug)]
pub struct KeyPair(dalek::Keypair);

impl signature::Signer<ed25519::Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<ed25519::Signature, ed25519::Error> {
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

    // for testing
    pub fn random_for_testing_only() -> Self {
        use rand::Rng;
        let random_bytes = rand::thread_rng().gen::<[u8; dalek::PUBLIC_KEY_LENGTH]>();
        Self(random_bytes)
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

pub type SuiAddress = PublicKeyBytes;
pub type AuthorityName = PublicKeyBytes;

// Define digests and object IDs. For now, ID's are the same as Move account addresses
// (16 bytes) for easy compatibility with Move. However, we'll probably want 20+ byte
// addresses, either by changing Move to allow different address lengths or by decoupling
// addresses and ID's
pub type ObjectID = AccountAddress;
pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

/// An object can be either owned by an account address, or another object.
// TODO: A few things to improve:
// 1. We may want to support multiple signing schemas, rename Authenticator to Address,
//    and rename the Address enum to Ed25519PublicKey, so that we could add more.
// 2. We may want to make Authenticator a fix-sized array instead of having different size
//    for different variants, through hashing.
// Refer details to https://github.com/MystenLabs/fastnft/pull/292.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Deserialize, PartialOrd, Ord, Serialize, Hash)]
pub enum Authenticator {
    Address(SuiAddress),
    Object(ObjectID),
}

impl Authenticator {
    pub fn is_address(&self, address: &SuiAddress) -> bool {
        match self {
            Self::Address(addr) => addr == address,
            Self::Object(_) => false,
        }
    }
}

// We use SHA3-256 hence 32 bytes here
const TRANSACTION_DIGEST_LENGTH: usize = 32;

pub const SEQUENCE_NUMBER_MAX: SequenceNumber = SequenceNumber(0x7fff_ffff_ffff_ffff);
pub const OBJECT_DIGEST_MAX: ObjectDigest = ObjectDigest([255; 32]);
pub const OBJECT_DIGEST_DELETED: ObjectDigest = ObjectDigest([99; 32]);

/// A transaction will have a (unique) digest.

#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct TransactionDigest(#[serde_as(as = "Bytes")] [u8; TRANSACTION_DIGEST_LENGTH]);
// Each object has a unique digest
#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct ObjectDigest(#[serde_as(as = "Bytes")] pub [u8; 32]); // We use SHA3-256 hence 32 bytes here

pub const TX_CONTEXT_MODULE_NAME: &IdentStr = ident_str!("TxContext");
pub const TX_CONTEXT_STRUCT_NAME: &IdentStr = TX_CONTEXT_MODULE_NAME;

#[derive(Debug, Deserialize, Serialize)]
pub struct TxContext {
    /// Signer/sender of the transaction
    sender: Vec<u8>,
    /// Digest of the current transaction
    digest: Vec<u8>,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl TxContext {
    pub fn new(sender: &SuiAddress, digest: TransactionDigest) -> Self {
        Self {
            sender: sender.to_vec(),
            digest: digest.0.to_vec(),
            ids_created: 0,
        }
    }

    /// Derive a globally unique object ID by hashing self.digest | self.ids_created
    pub fn fresh_id(&mut self) -> ObjectID {
        let id = self.digest().derive_id(self.ids_created);

        self.ids_created += 1;
        id
    }

    /// Return the transaction digest, to include in new objects
    pub fn digest(&self) -> TransactionDigest {
        TransactionDigest::new(self.digest.clone().try_into().unwrap())
    }

    pub fn to_vec(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    /// Updates state of the context instance. It's intended to use
    /// when mutable context is passed over some boundary via
    /// serialize/deserialize and this is the reason why this method
    /// consumes the other contex..
    pub fn update_state(&mut self, other: TxContext) -> Result<(), SuiError> {
        if self.sender != other.sender || self.digest != other.digest {
            return Err(SuiError::InvalidTxUpdate);
        }
        self.ids_created = other.ids_created;
        Ok(())
    }

    // for testing
    pub fn random_for_testing_only() -> Self {
        Self::new(
            &SuiAddress::random_for_testing_only(),
            TransactionDigest::random(),
        )
    }
}

impl TransactionDigest {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// A digest we use to signify the parent transaction was the genesis,
    /// ie. for an object there is no parent digest.
    ///
    /// TODO(https://github.com/MystenLabs/fastnft/issues/65): we can pick anything here    
    pub fn genesis() -> Self {
        Self::new([0; 32])
    }

    /// Create an ObjectID from `self` and `creation_num`.
    /// Caller is responsible for ensuring that `creation_num` is fresh
    pub fn derive_id(&self, creation_num: u64) -> ObjectID {
        // TODO(https://github.com/MystenLabs/fastnft/issues/58):audit ID derivation

        let mut hasher = Sha3_256::default();
        hasher.update(self.0);
        hasher.update(creation_num.to_le_bytes());
        let hash = hasher.finalize();

        // truncate into an ObjectID.
        AccountAddress::try_from(&hash[0..AccountAddress::LENGTH]).unwrap()
    }

    // for testing
    pub fn random() -> Self {
        use rand::Rng;
        let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
        Self::new(random_bytes)
    }
}

impl ObjectDigest {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// A marker that signifies the object is deleted.
    pub fn deleted() -> Self {
        OBJECT_DIGEST_DELETED
    }
}

// TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
pub fn get_key_pair() -> (SuiAddress, KeyPair) {
    let mut csprng = OsRng;
    let keypair = dalek::Keypair::generate(&mut csprng);
    (PublicKeyBytes(keypair.public.to_bytes()), KeyPair(keypair))
}

pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (SuiAddress, KeyPair) {
    let keypair = dalek::Keypair::from_bytes(bytes).unwrap();
    (PublicKeyBytes(keypair.public.to_bytes()), KeyPair(keypair))
}

pub fn address_as_hex<S>(key: &PublicKeyBytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&encode_address_hex(key))
}

pub fn address_from_hex<'de, D>(deserializer: D) -> Result<PublicKeyBytes, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_address_hex(&s).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub fn encode_address_hex(key: &PublicKeyBytes) -> String {
    hex::encode(&key.0[..])
}

pub fn decode_address_hex(s: &str) -> Result<PublicKeyBytes, hex::FromHexError> {
    let value = hex::decode(s)?;
    let mut address = [0u8; dalek::PUBLIC_KEY_LENGTH];
    address.copy_from_slice(&value[..dalek::PUBLIC_KEY_LENGTH]);
    Ok(PublicKeyBytes(address))
}

impl std::fmt::LowerHex for PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }

        Ok(())
    }
}

impl std::fmt::UpperHex for PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }

        Ok(())
    }
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

pub fn dbg_addr(name: u8) -> SuiAddress {
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
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for ObjectDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "o#{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for TransactionDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "t#{}", s)?;
        Ok(())
    }
}

// TODO: rename to version
impl SequenceNumber {
    pub fn new() -> Self {
        SequenceNumber(0)
    }

    pub fn max() -> Self {
        SEQUENCE_NUMBER_MAX
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub const fn from_u64(u: u64) -> Self {
        SequenceNumber(u)
    }

    #[must_use]
    pub fn increment(self) -> SequenceNumber {
        // TODO: Ensure this never overflow.
        // Option 1: Freeze the object when sequence number reaches MAX.
        // Option 2: Reject tx with MAX sequence number.
        // Issue #182.
        debug_assert_ne!(self.0, u64::MAX);
        Self(self.0 + 1)
    }

    pub fn decrement(self) -> Result<SequenceNumber, SuiError> {
        let val = self.0.checked_sub(1);
        match val {
            None => Err(SuiError::SequenceUnderflow),
            Some(val) => Ok(Self(val)),
        }
    }
}

impl From<SequenceNumber> for u64 {
    fn from(val: SequenceNumber) -> Self {
        val.0
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

    pub fn check<T>(&self, value: &T, author: SuiAddress) -> Result<(), SuiError>
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
        I: IntoIterator<Item = &'a (SuiAddress, Signature)>,
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

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

impl TryFrom<&[u8]> for TransactionDigest {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; TRANSACTION_DIGEST_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidTransactionDigest)?;
        Ok(Self(arr))
    }
}
