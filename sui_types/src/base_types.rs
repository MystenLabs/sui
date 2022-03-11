// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::crypto::PublicKeyBytes;
use crate::error::SuiError;
use ed25519_dalek::Digest;

use hex::FromHex;
use rand::Rng;
use serde::{de::Error as _, Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::fmt;

use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

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

pub type AuthorityName = PublicKeyBytes;

#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash)]
pub struct ObjectID(AccountAddress);

pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

pub const SUI_ADDRESS_LENGTH: usize = ObjectID::LENGTH;
#[serde_as]
#[derive(Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct SuiAddress(#[serde_as(as = "Bytes")] [u8; SUI_ADDRESS_LENGTH]);

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
        use sha2::Digest;
        let mut sha2 = sha2::Sha256::new();
        sha2.update(key.as_ref());
        let g_arr = sha2.finalize();

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
const TRANSACTION_DIGEST_LENGTH: usize = 32;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TxContext {
    /// Signer/sender of the transaction
    sender: AccountAddress,
    /// Digest of the current transaction
    digest: Vec<u8>,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl TxContext {
    pub fn new(sender: &SuiAddress, digest: TransactionDigest) -> Self {
        Self {
            sender: AccountAddress::new(sender.0),
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
        if self.sender != other.sender
            || self.digest != other.digest
            || other.ids_created < self.ids_created
        {
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
    let value = hex::decode(s)?;
    T::try_from(&value[..]).map_err(|_| anyhow::anyhow!("byte deserialization failed"))
}

impl std::fmt::LowerHex for SuiAddress {
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

impl std::fmt::UpperHex for SuiAddress {
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

impl fmt::Display for SuiAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:X}", self)
    }
}

pub fn address_as_base64<S>(address: &SuiAddress, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&encode_address(address))
}

pub fn address_from_base64<'de, D>(deserializer: D) -> Result<SuiAddress, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_address(&s).map_err(|err| serde::de::Error::custom(err.to_string()))?;
    Ok(value)
}

pub fn encode_address(address: &SuiAddress) -> String {
    base64::encode(&address.0[..])
}

pub fn decode_address(s: &str) -> Result<SuiAddress, anyhow::Error> {
    let value = base64::decode(s)?;
    let mut address = [0u8; SUI_ADDRESS_LENGTH];
    address.copy_from_slice(&value[..SUI_ADDRESS_LENGTH]);
    Ok(SuiAddress(address))
}

pub fn dbg_addr(name: u8) -> SuiAddress {
    let addr = [name; SUI_ADDRESS_LENGTH];
    SuiAddress(addr)
}

pub fn dbg_object_id(name: u8) -> ObjectID {
    ObjectID::from_bytes([name; ObjectID::LENGTH]).unwrap()
}

impl std::fmt::Debug for SuiAddress {
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
        self.0.fmt(f)
    }
}
impl fmt::Debug for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::LowerHex for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::UpperHex for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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
        match Self::from_hex(s.clone()) {
            Ok(q) => Ok(q),
            Err(_) => Self::from_hex_literal(&s),
        }
    }
}

impl std::str::FromStr for ObjectID {
    type Err = ObjectIDParseError;
    // Try to match both the literal (0xABC..) and the normal (ABC)
    fn from_str(s: &str) -> Result<Self, ObjectIDParseError> {
        match Self::from_hex(s) {
            Ok(q) => Ok(q),
            Err(_) => Self::from_hex_literal(s),
        }
    }
}

impl<'de> Deserialize<'de> for ObjectID {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = <String>::deserialize(deserializer)?;
            ObjectID::from_hex(s).map_err(D::Error::custom)
        } else {
            // In order to preserve the Serde data model and help analysis tools,
            // make sure to wrap our value in a container with the same name
            // as the original type.
            #[derive(::serde::Deserialize)]
            #[serde(rename = "ObjectID")]
            struct Value([u8; ObjectID::LENGTH]);

            let value = Value::deserialize(deserializer)?;
            Ok(ObjectID::new(value.0))
        }
    }
}

impl Serialize for ObjectID {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            self.to_hex().serialize(serializer)
        } else {
            // See comment in deserialize.
            serializer.serialize_newtype_struct("ObjectID", &self.0)
        }
    }
}
