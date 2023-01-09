// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use fastcrypto::encoding::decode_bytes_hex;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use rand::Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use std::borrow::Borrow;
use std::cmp::max;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;

pub use crate::committee::EpochId;
use crate::crypto::{
    AuthorityPublicKey, AuthorityPublicKeyBytes, KeypairTraits, PublicKey, SuiPublicKey,
};
use crate::error::ExecutionError;
use crate::error::ExecutionErrorKind;
use crate::error::SuiError;
use crate::gas_coin::GasCoin;
use crate::multisig::MultiPublicKey;
use crate::object::{Object, Owner};
use crate::sui_serde::Readable;
use fastcrypto::encoding::{Base58, Base64, Encoding, Hex};
use fastcrypto::hash::{HashFunction, Sha3_256};

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

pub type TxSequenceNumber = u64;

impl fmt::Display for SequenceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

pub type VersionNumber = SequenceNumber;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct UserData(pub Option<[u8; 32]>);

pub type AuthorityName = AuthorityPublicKeyBytes;

#[serde_as]
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ObjectID(
    #[schemars(with = "Hex")]
    #[serde_as(as = "Readable<Hex, _>")]
    AccountAddress,
);

pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

pub fn random_object_ref() -> ObjectRef {
    (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    )
}

/// Type of a Sui object
#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum ObjectType {
    /// Move package containing one or more bytecode modules
    Package,
    /// A Move struct of the given
    Struct(StructTag),
}

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct ObjectInfo {
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub type_: ObjectType,
    pub owner: Owner,
    pub previous_transaction: TransactionDigest,
}

impl ObjectInfo {
    pub fn new(oref: &ObjectRef, o: &Object) -> Self {
        let (object_id, version, digest) = *oref;
        let type_ = o
            .data
            .type_()
            .map(|tag| ObjectType::Struct(tag.clone()))
            .unwrap_or(ObjectType::Package);
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

impl ObjectType {
    pub fn is_gas_coin(&self) -> bool {
        match self {
            ObjectType::Struct(s) => s == &GasCoin::type_(),
            ObjectType::Package => false,
        }
    }
}

impl From<ObjectInfo> for ObjectRef {
    fn from(info: ObjectInfo) -> Self {
        (info.object_id, info.version, info.digest)
    }
}

impl From<&ObjectInfo> for ObjectRef {
    fn from(info: &ObjectInfo) -> Self {
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
    pub const ZERO: Self = Self([0u8; SUI_ADDRESS_LENGTH]);

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
        serializer.serialize_str(&key.map(Hex::encode).unwrap_or_default())
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

impl From<AccountAddress> for SuiAddress {
    fn from(address: AccountAddress) -> SuiAddress {
        Self(address.into_bytes())
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

impl From<&AuthorityPublicKeyBytes> for SuiAddress {
    fn from(pkb: &AuthorityPublicKeyBytes) -> Self {
        let mut hasher = Sha3_256::default();
        hasher.update([AuthorityPublicKey::SIGNATURE_SCHEME.flag()]);
        hasher.update(pkb);
        let g_arr = hasher.finalize();

        let mut res = [0u8; SUI_ADDRESS_LENGTH];
        // OK to access slice because Sha3_256 should never be shorter than SUI_ADDRESS_LENGTH.
        res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
        SuiAddress(res)
    }
}

impl<T: SuiPublicKey> From<&T> for SuiAddress {
    fn from(pk: &T) -> Self {
        let mut hasher = Sha3_256::default();
        hasher.update([T::SIGNATURE_SCHEME.flag()]);
        hasher.update(pk);
        let g_arr = hasher.finalize();

        let mut res = [0u8; SUI_ADDRESS_LENGTH];
        // OK to access slice because Sha3_256 should never be shorter than SUI_ADDRESS_LENGTH.
        res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
        SuiAddress(res)
    }
}

impl From<&PublicKey> for SuiAddress {
    fn from(pk: &PublicKey) -> Self {
        let mut hasher = Sha3_256::default();
        hasher.update([pk.flag()]);
        hasher.update(pk);
        let g_arr = hasher.finalize();

        let mut res = [0u8; SUI_ADDRESS_LENGTH];
        // OK to access slice because Sha3_256 should never be shorter than SUI_ADDRESS_LENGTH.
        res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
        SuiAddress(res)
    }
}

/// A Multisig address is the first 20 bytes of `threshold || pk_1 || weight_1 || ... || pk_n || weight_n`
/// of all participating public keys and its weight.  
impl From<MultiPublicKey> for SuiAddress {
    fn from(multi_pk: MultiPublicKey) -> Self {
        let mut hasher = Sha3_256::default();
        hasher.update(multi_pk.threshold().to_le_bytes());
        multi_pk.pubkeys().iter().for_each(|(pk, w)| {
            hasher.update(pk.as_ref());
            hasher.update(w.to_le_bytes());
        });
        let g_arr = hasher.finalize();

        let mut res = [0u8; SUI_ADDRESS_LENGTH];
        // OK to access slice because Sha3_256 should never be shorter than SUI_ADDRESS_LENGTH.
        res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
        SuiAddress(res)
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
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, Bytes>")]
    [u8; TRANSACTION_DIGEST_LENGTH],
);

// Each object has a unique digest
#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ObjectDigest(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; OBJECT_DIGEST_LENGTH],
); // We use SHA3-256 hence 32 bytes here

#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionEffectsDigest(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; TRANSACTION_DIGEST_LENGTH],
);

impl TransactionEffectsDigest {
    pub const ZERO: Self = TransactionEffectsDigest([0u8; TRANSACTION_DIGEST_LENGTH]);

    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; TRANSACTION_DIGEST_LENGTH]>();
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

pub const STD_OPTION_MODULE_NAME: &IdentStr = ident_str!("option");
pub const STD_OPTION_STRUCT_NAME: &IdentStr = ident_str!("Option");

pub const STD_ASCII_MODULE_NAME: &IdentStr = ident_str!("ascii");
pub const STD_ASCII_STRUCT_NAME: &IdentStr = ident_str!("String");

pub const STD_UTF8_MODULE_NAME: &IdentStr = ident_str!("string");
pub const STD_UTF8_STRUCT_NAME: &IdentStr = ident_str!("String");

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

    // for testing
    pub fn with_sender_for_testing_only(sender: &SuiAddress) -> Self {
        Self::new(sender, &TransactionDigest::random(), 0)
    }
}

impl TransactionDigest {
    pub fn new(bytes: [u8; TRANSACTION_DIGEST_LENGTH]) -> Self {
        Self(bytes)
    }

    /// A digest we use to signify the parent transaction was the genesis,
    /// ie. for an object there is no parent digest.
    // TODO(https://github.com/MystenLabs/sui/issues/65): we can pick anything here
    pub fn genesis() -> Self {
        Self::new([0; TRANSACTION_DIGEST_LENGTH])
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
        // OK to access slice because Sha3_256 should never be shorter than ObjectID::LENGTH.
        ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap()
    }

    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; TRANSACTION_DIGEST_LENGTH]>();
        Self::new(random_bytes)
    }

    /// Translates digest into a Vec of bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn into_bytes(self) -> [u8; TRANSACTION_DIGEST_LENGTH] {
        self.0
    }

    pub fn encode(&self) -> String {
        Base64::encode(self.0)
    }

    // TODO: de-dup this
    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }
}

impl AsRef<[u8]> for TransactionDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
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
    pub const MIN: ObjectDigest = ObjectDigest([u8::MIN; OBJECT_DIGEST_LENGTH]);
    pub const MAX: ObjectDigest = ObjectDigest([u8::MAX; OBJECT_DIGEST_LENGTH]);
    pub const OBJECT_DIGEST_DELETED_BYTE_VAL: u8 = 99;
    pub const OBJECT_DIGEST_WRAPPED_BYTE_VAL: u8 = 88;

    /// A marker that signifies the object is deleted.
    pub const OBJECT_DIGEST_DELETED: ObjectDigest =
        ObjectDigest([Self::OBJECT_DIGEST_DELETED_BYTE_VAL; OBJECT_DIGEST_LENGTH]);

    /// A marker that signifies the object is wrapped into another object.
    pub const OBJECT_DIGEST_WRAPPED: ObjectDigest =
        ObjectDigest([Self::OBJECT_DIGEST_WRAPPED_BYTE_VAL; OBJECT_DIGEST_LENGTH]);

    pub fn new(bytes: [u8; OBJECT_DIGEST_LENGTH]) -> Self {
        Self(bytes)
    }

    pub fn is_alive(&self) -> bool {
        *self != Self::OBJECT_DIGEST_DELETED && *self != Self::OBJECT_DIGEST_WRAPPED
    }

    // for testing
    pub fn random() -> Self {
        let random_bytes = rand::thread_rng().gen::<[u8; OBJECT_DIGEST_LENGTH]>();
        Self::new(random_bytes)
    }

    pub fn encode(&self) -> String {
        Base64::encode(self.0)
    }
}

pub fn get_new_address<K: KeypairTraits>() -> SuiAddress
where
    <K as KeypairTraits>::PubKey: SuiPublicKey,
{
    crate::crypto::get_key_pair::<K>().0
}

pub fn bytes_as_hex<B, S>(bytes: B, serializer: S) -> Result<S::Ok, S::Error>
where
    B: AsRef<[u8]>,
    S: serde::ser::Serializer,
{
    serializer.serialize_str(&Hex::encode(bytes))
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
        write!(f, "{}", Hex::encode(self))
    }
}

impl fmt::UpperHex for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", Hex::encode(self).to_uppercase())
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
        let s = Hex::encode(self.0);
        write!(f, "o#{}", s)?;
        Ok(())
    }
}

impl AsRef<[u8]> for ObjectDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl fmt::Display for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self)
    }
}

impl fmt::LowerHex for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", Hex::encode(self))
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
        let s = Base58::encode(self.0);
        write!(f, "{}", s)?;
        Ok(())
    }
}

impl std::fmt::Debug for TransactionEffectsDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = Base64::encode(self.0);
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

    pub fn increment_to(&mut self, next: SequenceNumber) {
        debug_assert!(*self < next, "Not an increment: {} to {}", self, next);
        *self = next;
    }

    pub fn decrement_to(&mut self, prev: SequenceNumber) {
        debug_assert!(prev < *self, "Not a decrement: {} to {}", self, prev);
        *self = prev;
    }

    /// Returns a new sequence number that is greater than all `SequenceNumber`s in `inputs`,
    /// assuming this operation will not overflow.
    #[must_use]
    pub fn lamport_increment(inputs: impl IntoIterator<Item = SequenceNumber>) -> SequenceNumber {
        let max_input = inputs.into_iter().fold(SequenceNumber::new(), max);

        // TODO: Ensure this never overflows.
        // Option 1: Freeze the object when sequence number reaches MAX.
        // Option 2: Reject tx with MAX sequence number.
        // Issue #182.
        assert_ne!(max_input.0, u64::MAX);

        SequenceNumber(max_input.0 + 1)
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
    pub const MAX: Self = Self::new([0xff; Self::LENGTH]);
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
            Self::from_str(&hex_str)
        } else {
            Self::from_str(&literal[2..])
        }
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

#[derive(PartialEq, Eq, Clone, Debug, thiserror::Error)]
pub enum ObjectIDParseError {
    #[error("ObjectID hex literal must start with 0x")]
    HexLiteralPrefixMissing,

    #[error("ObjectID hex string should only contain 0-9, A-F, a-f")]
    InvalidHexCharacter,

    #[error("hex string must be even-numbered. Two chars maps to one byte.")]
    OddLength,

    #[error("ObjectID must be {} bytes long.", ObjectID::LENGTH)]
    InvalidLength,

    #[error("Could not convert from bytes slice")]
    TryFromSliceError,
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

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Package => write!(f, "Package"),
            ObjectType::Struct(t) => write!(f, "{}", t),
        }
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
        write!(f, "{}", Hex::encode(self))
    }
}

impl fmt::UpperHex for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", Hex::encode(self).to_uppercase())
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
        Self::from_str(&s).or_else(|_| Self::from_hex_literal(&s))
    }
}

impl FromStr for SuiAddress {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        decode_bytes_hex(s).map_err(|e| anyhow!(e))
    }
}

impl FromStr for ObjectID {
    type Err = ObjectIDParseError;

    fn from_str(s: &str) -> Result<Self, ObjectIDParseError> {
        // Try to match both the literal (0xABC..) and the normal (ABC)
        decode_bytes_hex(s).or_else(|_| Self::from_hex_literal(s))
    }
}

impl FromStr for TransactionDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0u8; TRANSACTION_DIGEST_LENGTH];
        result.copy_from_slice(&Base58::decode(s).map_err(|e| anyhow!(e))?);
        Ok(TransactionDigest(result))
    }
}
