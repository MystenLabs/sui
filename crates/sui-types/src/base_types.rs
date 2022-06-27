// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;

use anyhow::anyhow;
use base64ct::Encoding;
use curve25519_dalek::ristretto::RistrettoPoint;
use digest::Digest;
use ed25519_dalek::Sha512;
use hex::FromHex;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use opentelemetry::{global, Context};
use rand::Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use sha3::Sha3_256;

use crate::committee::EpochId;
use crate::crypto::PublicKeyBytes;
use crate::error::ExecutionError;
use crate::error::ExecutionErrorKind;
use crate::error::SuiError;
use crate::object::{Object, Owner};
use crate::sui_serde::Base64;
use crate::sui_serde::Hex;
use crate::sui_serde::Readable;
use crate::waypoint::IntoPoint;

#[cfg(test)]
#[path = "unit_tests/base_types_tests.rs"]
mod base_types_tests;

#[derive(
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Copy,
    Clone,
    Hash,
    Default,
    Debug,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub struct SequenceNumber(u64);

impl fmt::Display for SequenceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

pub type VersionNumber = SequenceNumber;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct UserData(pub Option<[u8; 32]>);

pub type AuthorityName = PublicKeyBytes;

#[serde_as]
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ObjectID(
    #[schemars(with = "Hex")]
    #[serde_as(as = "Readable<Hex, _>")]
    AccountAddress,
);

pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct ObjectInfo {
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub type_: String,
    pub owner: Owner,
    pub previous_transaction: TransactionDigest,
}

impl ObjectInfo {
    pub fn new(oref: &ObjectRef, o: &Object) -> Self {
        let (object_id, version, digest) = *oref;
        let type_ = o
            .data
            .type_()
            .map(|tag| tag.to_string())
            .unwrap_or_else(|| "Package".to_string());
        Self {
            object_id,
            version,
            digest,
            type_,
            owner: o.owner,
            previous_transaction: o.previous_transaction,
        }
    }
}

impl From<ObjectInfo> for ObjectRef {
    fn from(info: ObjectInfo) -> Self {
        (info.object_id, info.version, info.digest)
    }
}

pub const SUI_ADDRESS_LENGTH: usize = ObjectID::LENGTH;

#[serde_as]
#[derive(
    Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct SuiAddress(
    #[schemars(with = "Hex")]
    #[serde_as(as = "Readable<Hex, _>")]
    [u8; SUI_ADDRESS_LENGTH],
);

impl SuiAddress {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    // for testing
    pub fn random_for_testing_only() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; SUI_ADDRESS_LENGTH]>();
        Self(random_bytes)
    }

    pub fn optional_address_as_hex<S>(
        key: &Option<SuiAddress>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(
            &*key
                .map(|addr| encode_bytes_hex(&addr))
                .unwrap_or_else(|| "".to_string()),
        )
    }

    pub fn optional_address_from_hex<'de, D>(
        deserializer: D,
    ) -> Result<Option<SuiAddress>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = decode_bytes_hex(&s).map_err(serde::de::Error::custom)?;
        Ok(Some(value))
    }

    pub fn to_inner(self) -> [u8; SUI_ADDRESS_LENGTH] {
        self.0
    }
}

impl From<ObjectID> for SuiAddress {
    fn from(object_id: ObjectID) -> SuiAddress {
        Self(object_id.into_bytes())
    }
}

impl TryFrom<Vec<u8>> for SuiAddress {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        let arr: [u8; SUI_ADDRESS_LENGTH] =
            bytes.try_into().map_err(|_| SuiError::InvalidAddress)?;
        Ok(Self(arr))
    }
}

impl From<&PublicKeyBytes> for SuiAddress {
    fn from(key: &PublicKeyBytes) -> SuiAddress {
        Self::from(*key)
    }
}

impl From<PublicKeyBytes> for SuiAddress {
    fn from(key: PublicKeyBytes) -> SuiAddress {
        let mut hasher = Sha3_256::default();
        hasher.update(key.as_ref());
        let g_arr = hasher.finalize();

        let mut res = [0u8; SUI_ADDRESS_LENGTH];
        res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
        Self(res)
    }
}

impl TryFrom<&[u8]> for SuiAddress {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; SUI_ADDRESS_LENGTH] =
            bytes.try_into().map_err(|_| SuiError::InvalidAddress)?;
        Ok(Self(arr))
    }
}

impl AsRef<[u8]> for SuiAddress {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

// We use SHA3-256 hence 32 bytes here
pub const TRANSACTION_DIGEST_LENGTH: usize = 32;
pub const OBJECT_DIGEST_LENGTH: usize = 32;

/// A transaction will have a (unique) digest.
#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionDigest(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; TRANSACTION_DIGEST_LENGTH],
);

impl IntoPoint for TransactionDigest {
    fn into_point(&self) -> RistrettoPoint {
        RistrettoPoint::hash_from_bytes::<Sha512>(&self.0)
    }
}

// Each object has a unique digest
#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ObjectDigest(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; 32],
); // We use SHA3-256 hence 32 bytes here

#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionEffectsDigest(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; TRANSACTION_DIGEST_LENGTH],
);

impl TransactionEffectsDigest {
    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
        Self(random_bytes)
    }
}

#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema, Debug,
)]
pub struct ExecutionDigests {
    pub transaction: TransactionDigest,
    pub effects: TransactionEffectsDigest,
}

impl ExecutionDigests {
    pub fn new(transaction: TransactionDigest, effects: TransactionEffectsDigest) -> Self {
        Self {
            transaction,
            effects,
        }
    }

    pub fn random() -> Self {
        Self {
            transaction: TransactionDigest::random(),
            effects: TransactionEffectsDigest::random(),
        }
    }
}

impl IntoPoint for ExecutionDigests {
    fn into_point(&self) -> RistrettoPoint {
        let mut data = [0; 64];
        data[0..32].clone_from_slice(&self.transaction.0);
        data[32..64].clone_from_slice(&self.effects.0);
        RistrettoPoint::from_uniform_bytes(&data)
    }
}

pub const STD_OPTION_MODULE_NAME: &IdentStr = ident_str!("option");
pub const STD_OPTION_STRUCT_NAME: &IdentStr = ident_str!("Option");

pub const TX_CONTEXT_MODULE_NAME: &IdentStr = ident_str!("tx_context");
pub const TX_CONTEXT_STRUCT_NAME: &IdentStr = ident_str!("TxContext");

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TxContext {
    /// Signer/sender of the transaction
    sender: AccountAddress,
    /// Digest of the current transaction
    digest: Vec<u8>,
    /// The current epoch number
    epoch: EpochId,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl TxContext {
    pub fn new(sender: &SuiAddress, digest: &TransactionDigest, epoch: EpochId) -> Self {
        Self {
            sender: AccountAddress::new(sender.0),
            digest: digest.0.to_vec(),
            epoch,
            ids_created: 0,
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
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

    pub fn sender(&self) -> SuiAddress {
        SuiAddress::from(ObjectID(self.sender))
    }

    pub fn to_vec(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    /// Updates state of the context instance. It's intended to use
    /// when mutable context is passed over some boundary via
    /// serialize/deserialize and this is the reason why this method
    /// consumes the other context..
    pub fn update_state(&mut self, other: TxContext) -> Result<(), ExecutionError> {
        if self.sender != other.sender
            || self.digest != other.digest
            || other.ids_created < self.ids_created
        {
            return Err(ExecutionErrorKind::InvalidTransactionUpdate.into());
        }
        self.ids_created = other.ids_created;
        Ok(())
    }

    // for testing
    pub fn random_for_testing_only() -> Self {
        Self::new(
            &SuiAddress::random_for_testing_only(),
            &TransactionDigest::random(),
            0,
        )
    }

    /// A function that lists all IDs created by this TXContext
    pub fn recreate_all_ids(&self) -> HashSet<ObjectID> {
        (0..self.ids_created)
            .map(|seq| self.digest().derive_id(seq))
            .collect()
    }
}

impl TransactionDigest {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// A digest we use to signify the parent transaction was the genesis,
    /// ie. for an object there is no parent digest.
    ///
    /// TODO(https://github.com/MystenLabs/sui/issues/65): we can pick anything here
    pub fn genesis() -> Self {
        Self::new([0; 32])
    }

    /// Create an ObjectID from `self` and `creation_num`.
    /// Caller is responsible for ensuring that `creation_num` is fresh
    pub fn derive_id(&self, creation_num: u64) -> ObjectID {
        // TODO(https://github.com/MystenLabs/sui/issues/58):audit ID derivation

        let mut hasher = Sha3_256::default();
        hasher.update(self.0);
        hasher.update(creation_num.to_le_bytes());
        let hash = hasher.finalize();

        // truncate into an ObjectID.
        ObjectID::try_from(&hash[0..ObjectID::LENGTH]).unwrap()
    }

    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
        Self::new(random_bytes)
    }

    /// Translates digest into a Vec of bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl AsRef<[u8]> for TransactionDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Returns a Context for OpenTelemetry tracing from a TransactionDigest
// NOTE: See https://github.com/MystenLabs/sui/issues/852
// The current code doesn't really work.  Maybe the traceparent needs to be a specific format,
// prohably needs to be the propagated trace ID from the source.
pub fn context_from_digest(digest: TransactionDigest) -> Context {
    // TODO: don't create a HashMap, that wastes memory and costs an allocation!
    let mut carrier = HashMap::new();
    // TODO: figure out exactly what key to use.  I suspect it has to be the parent span ID in OpenTelemetry format.
    carrier.insert("traceparent".to_string(), hex::encode(digest.0));
    // carrier.insert("tx_digest".to_string(), hex::encode(digest.0));

    global::get_text_map_propagator(|propagator| propagator.extract(&carrier))
}

impl Borrow<[u8]> for TransactionDigest {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl Borrow<[u8]> for &TransactionDigest {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl ObjectDigest {
    pub const MIN: ObjectDigest = ObjectDigest([u8::MIN; 32]);
    pub const MAX: ObjectDigest = ObjectDigest([u8::MAX; 32]);

    /// A marker that signifies the object is deleted.
    pub const OBJECT_DIGEST_DELETED: ObjectDigest = ObjectDigest([99; 32]);

    /// A marker that signifies the object is wrapped into another object.
    pub const OBJECT_DIGEST_WRAPPED: ObjectDigest = ObjectDigest([88; 32]);

    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn is_alive(&self) -> bool {
        *self != Self::OBJECT_DIGEST_DELETED && *self != Self::OBJECT_DIGEST_WRAPPED
    }

    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
        Self::new(random_bytes)
    }
}

pub fn get_new_address() -> SuiAddress {
    crate::crypto::get_key_pair().0
}

pub fn bytes_as_hex<B, S>(bytes: &B, serializer: S) -> Result<S::Ok, S::Error>
where
    B: AsRef<[u8]>,
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&encode_bytes_hex(bytes))
}

pub fn bytes_from_hex<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: for<'a> TryFrom<&'a [u8]>,
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_bytes_hex(&s).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub fn encode_bytes_hex<B: AsRef<[u8]>>(bytes: &B) -> String {
    hex::encode(bytes.as_ref())
}

pub fn decode_bytes_hex<T: for<'a> TryFrom<&'a [u8]>>(s: &str) -> Result<T, anyhow::Error> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let value = hex::decode(s)?;
    T::try_from(&value[..]).map_err(|_| anyhow::anyhow!("byte deserialization failed"))
}

impl fmt::Display for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self)
    }
}

impl fmt::Debug for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self)
    }
}

impl fmt::LowerHex for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", encode_bytes_hex(self))
    }
}

impl fmt::UpperHex for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", encode_bytes_hex(self).to_uppercase())
    }
}

pub fn dbg_addr(name: u8) -> SuiAddress {
    let addr = [name; SUI_ADDRESS_LENGTH];
    SuiAddress(addr)
}

pub fn dbg_object_id(name: u8) -> ObjectID {
    ObjectID::from_bytes([name; ObjectID::LENGTH]).unwrap()
}

impl std::fmt::Debug for ObjectDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "o#{}", s)?;
        Ok(())
    }
}

impl AsRef<[u8]> for ObjectDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl TryFrom<&[u8]> for ObjectDigest {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; OBJECT_DIGEST_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidTransactionDigest)?;
        Ok(Self(arr))
    }
}

impl std::fmt::Debug for TransactionDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(&self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for TransactionEffectsDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(&self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

// TODO: rename to version
impl SequenceNumber {
    pub const MIN: SequenceNumber = SequenceNumber(u64::MIN);
    pub const MAX: SequenceNumber = SequenceNumber(0x7fff_ffff_ffff_ffff);

    pub fn new() -> Self {
        SequenceNumber(0)
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

impl TryFrom<&[u8]> for TransactionDigest {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; TRANSACTION_DIGEST_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidTransactionDigest)?;
        Ok(Self(arr))
    }
}

impl ObjectID {
    /// The number of bytes in an address.
    pub const LENGTH: usize = AccountAddress::LENGTH;
    /// Hex address: 0x0
    pub const ZERO: Self = Self::new([0u8; Self::LENGTH]);
    /// Creates a new ObjectID
    pub const fn new(obj_id: [u8; Self::LENGTH]) -> Self {
        Self(AccountAddress::new(obj_id))
    }

    /// Random ObjectID
    pub fn random() -> Self {
        Self::from(AccountAddress::random())
    }

    // Random for testing
    pub fn random_from_rng<R>(rng: &mut R) -> Self
    where
        R: rand::CryptoRng + rand::RngCore,
    {
        let buf: [u8; Self::LENGTH] = rng.gen();
        ObjectID::new(buf)
    }

    pub const fn from_single_byte(byte: u8) -> ObjectID {
        let mut bytes = [0u8; Self::LENGTH];
        bytes[Self::LENGTH - 1] = byte;
        ObjectID::new(bytes)
    }

    /// Converts from hex string to ObjectID where the string is prefixed with 0x
    /// Its okay if the strings are less than expected
    pub fn from_hex_literal(literal: &str) -> Result<Self, ObjectIDParseError> {
        if !literal.starts_with("0x") {
            return Err(ObjectIDParseError::HexLiteralPrefixMissing);
        }

        let hex_len = literal.len() - 2;

        // If the string is too short, pad it
        if hex_len < Self::LENGTH * 2 {
            let mut hex_str = String::with_capacity(Self::LENGTH * 2);
            for _ in 0..Self::LENGTH * 2 - hex_len {
                hex_str.push('0');
            }
            hex_str.push_str(&literal[2..]);
            Self::from_hex(hex_str)
        } else {
            Self::from_hex(&literal[2..])
        }
    }

    pub fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, ObjectIDParseError> {
        <[u8; Self::LENGTH]>::from_hex(hex)
            .map_err(ObjectIDParseError::from)
            .map(ObjectID::from)
    }

    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Self, ObjectIDParseError> {
        <[u8; Self::LENGTH]>::try_from(bytes.as_ref())
            .map_err(|_| ObjectIDParseError::TryFromSliceError)
            .map(ObjectID::from)
    }

    /// Incremenent the ObjectID by usize IDs, assuming the ObjectID hex is a number represented as an array of bytes
    pub fn advance(&self, step: usize) -> Result<ObjectID, anyhow::Error> {
        let mut curr_vec = self.as_slice().to_vec();
        let mut step_copy = step;

        let mut carry = 0;
        for idx in (0..Self::LENGTH).rev() {
            if step_copy == 0 {
                // Nothing else to do
                break;
            }
            // Extract the relevant part
            let g = (step_copy % 0x100) as u16;
            // Shift to next group
            step_copy >>= 8;
            let mut val = curr_vec[idx] as u16;
            (carry, val) = ((val + carry + g) / 0x100, (val + carry + g) % 0x100);
            curr_vec[idx] = val as u8;
        }

        if carry > 0 {
            return Err(anyhow!("Increment will cause overflow"));
        }
        ObjectID::from_bytes(curr_vec).map_err(|w| w.into())
    }

    /// Incremenent the ObjectID by one, assuming the ObjectID hex is a number represented as an array of bytes
    pub fn next_increment(&self) -> Result<ObjectID, anyhow::Error> {
        let mut prev_val = self.as_slice().to_vec();
        let mx = [0xFF; Self::LENGTH];

        if prev_val == mx {
            return Err(anyhow!("Increment will cause overflow"));
        }

        // This logic increments the integer representation of an ObjectID u8 array
        for idx in (0..Self::LENGTH).rev() {
            if prev_val[idx] == 0xFF {
                prev_val[idx] = 0;
            } else {
                prev_val[idx] += 1;
                break;
            };
        }
        ObjectID::from_bytes(prev_val.clone()).map_err(|w| w.into())
    }

    /// Create `count` object IDs starting with one at `offset`
    pub fn in_range(offset: ObjectID, count: u64) -> Result<Vec<ObjectID>, anyhow::Error> {
        let mut ret = Vec::new();
        let mut prev = offset;
        for o in 0..count {
            if o != 0 {
                prev = prev.next_increment()?;
            }
            ret.push(prev);
        }
        Ok(ret)
    }
}

#[derive(PartialEq, Clone, Debug, thiserror::Error)]
pub enum ObjectIDParseError {
    #[error("ObjectID hex literal must start with 0x")]
    HexLiteralPrefixMissing,

    #[error("{err} (ObjectID hex string should only contain 0-9, A-F, a-f)")]
    InvalidHexCharacter { err: hex::FromHexError },

    #[error("{err} (hex string must be even-numbered. Two chars maps to one byte).")]
    OddLength { err: hex::FromHexError },

    #[error("{err} (ObjectID must be {} bytes long).", ObjectID::LENGTH)]
    InvalidLength { err: hex::FromHexError },

    #[error("Could not convert from bytes slice")]
    TryFromSliceError,
    // #[error("Internal hex parser error: {err}")]
    // HexParserError { err: hex::FromHexError },
}

/// Wraps the underlying parsing errors
impl From<hex::FromHexError> for ObjectIDParseError {
    fn from(err: hex::FromHexError) -> Self {
        match err {
            hex::FromHexError::InvalidHexCharacter { c, index } => {
                ObjectIDParseError::InvalidHexCharacter {
                    err: hex::FromHexError::InvalidHexCharacter { c, index },
                }
            }
            hex::FromHexError::OddLength => ObjectIDParseError::OddLength {
                err: hex::FromHexError::OddLength,
            },
            hex::FromHexError::InvalidStringLength => ObjectIDParseError::InvalidLength {
                err: hex::FromHexError::InvalidStringLength,
            },
        }
    }
}

impl From<[u8; ObjectID::LENGTH]> for ObjectID {
    fn from(bytes: [u8; ObjectID::LENGTH]) -> Self {
        Self::new(bytes)
    }
}

impl From<SuiAddress> for ObjectID {
    fn from(address: SuiAddress) -> ObjectID {
        let tmp: AccountAddress = address.into();
        tmp.into()
    }
}

impl std::ops::Deref for ObjectID {
    type Target = AccountAddress;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<AccountAddress> for ObjectID {
    fn from(address: AccountAddress) -> Self {
        Self(address)
    }
}

impl From<ObjectID> for AccountAddress {
    fn from(obj_id: ObjectID) -> Self {
        obj_id.0
    }
}

impl From<SuiAddress> for AccountAddress {
    fn from(address: SuiAddress) -> Self {
        Self::new(address.0)
    }
}

impl fmt::Display for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self)
    }
}

impl fmt::Debug for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self)
    }
}

impl fmt::LowerHex for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", encode_bytes_hex(self))
    }
}

impl fmt::UpperHex for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", encode_bytes_hex(self).to_uppercase())
    }
}

impl AsRef<[u8]> for ObjectID {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl TryFrom<&[u8]> for ObjectID {
    type Error = ObjectIDParseError;

    /// Tries to convert the provided byte array into ObjectID.
    fn try_from(bytes: &[u8]) -> Result<ObjectID, ObjectIDParseError> {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<Vec<u8>> for ObjectID {
    type Error = ObjectIDParseError;

    /// Tries to convert the provided byte buffer into ObjectID.
    fn try_from(bytes: Vec<u8>) -> Result<ObjectID, ObjectIDParseError> {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<String> for ObjectID {
    type Error = ObjectIDParseError;

    fn try_from(s: String) -> Result<ObjectID, ObjectIDParseError> {
        Self::from_hex(s.clone()).or_else(|_| Self::from_hex_literal(&s))
    }
}

impl FromStr for SuiAddress {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        decode_bytes_hex(s)
    }
}

impl std::str::FromStr for ObjectID {
    type Err = ObjectIDParseError;

    fn from_str(s: &str) -> Result<Self, ObjectIDParseError> {
        // Try to match both the literal (0xABC..) and the normal (ABC)
        Self::from_hex(s).or_else(|_| Self::from_hex_literal(s))
    }
}
