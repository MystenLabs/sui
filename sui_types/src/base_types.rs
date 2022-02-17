use crate::crypto::PublicKeyBytes;
// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::error::SuiError;
use ed25519_dalek::Digest;

use std::convert::{TryFrom, TryInto};

use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

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

pub type AuthorityName = PublicKeyBytes;

// TODO: Have ObjectID wrap AccountAddress instead of type alias.
pub type ObjectID = AccountAddress;
pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

pub const SUI_ADDRESS_LENGTH: usize = 32;
#[serde_as]
#[derive(Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct SuiAddress(#[serde_as(as = "Bytes")] [u8; SUI_ADDRESS_LENGTH]);

impl SuiAddress {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    // for testing
    pub fn random_for_testing_only() -> Self {
        use rand::Rng;
        let random_bytes = rand::thread_rng().gen::<[u8; SUI_ADDRESS_LENGTH]>();
        Self(random_bytes)
    }
}

impl From<ObjectID> for SuiAddress {
    fn from(object_id: ObjectID) -> SuiAddress {
        // TODO: Use proper hashing to convert ObjectID to SuiAddress
        let mut address = [0u8; SUI_ADDRESS_LENGTH];
        address[..AccountAddress::LENGTH].clone_from_slice(&object_id.into_bytes());
        Self(address)
    }
}

impl From<&PublicKeyBytes> for SuiAddress {
    fn from(key: &PublicKeyBytes) -> SuiAddress {
        let mut sha3 = Sha3_256::new();
        sha3.update(key.as_ref());
        let g_arr = sha3.finalize();
        Self(*(g_arr.as_ref()))
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

impl From<Vec<u8>> for SuiAddress {
    fn from(bytes: Vec<u8>) -> Self {
        let mut result = [0u8; SUI_ADDRESS_LENGTH];
        result.copy_from_slice(&bytes[..SUI_ADDRESS_LENGTH]);
        Self(result)
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
    T: From<Vec<u8>>,
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_bytes_hex(&s).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub fn encode_bytes_hex<B: AsRef<[u8]>>(bytes: &B) -> String {
    hex::encode(bytes.as_ref())
}

pub fn decode_bytes_hex<T: From<Vec<u8>>>(s: &str) -> Result<T, hex::FromHexError> {
    let value = hex::decode(s)?;
    Ok(value.into())
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
    let mut address = [0u8; ed25519_dalek::PUBLIC_KEY_LENGTH];
    address.copy_from_slice(&value[..ed25519_dalek::PUBLIC_KEY_LENGTH]);
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

impl TryFrom<&[u8]> for TransactionDigest {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; TRANSACTION_DIGEST_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidTransactionDigest)?;
        Ok(Self(arr))
    }
}
