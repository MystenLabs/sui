// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::coin::Coin;
use crate::coin::CoinMetadata;
use crate::coin::COIN_MODULE_NAME;
use crate::coin::COIN_STRUCT_NAME;
pub use crate::committee::EpochId;
use crate::crypto::{
    AuthorityPublicKeyBytes, DefaultHash, PublicKey, SignatureScheme, SuiPublicKey, SuiSignature,
};
pub use crate::digests::{ObjectDigest, TransactionDigest, TransactionEffectsDigest};
use crate::dynamic_field::DynamicFieldInfo;
use crate::dynamic_field::DynamicFieldType;
use crate::effects::TransactionEffects;
use crate::effects::TransactionEffectsAPI;
use crate::epoch_data::EpochData;
use crate::error::ExecutionErrorKind;
use crate::error::SuiError;
use crate::error::{ExecutionError, SuiResult};
use crate::gas_coin::GasCoin;
use crate::gas_coin::GAS;
use crate::governance::StakedSui;
use crate::governance::STAKED_SUI_STRUCT_NAME;
use crate::governance::STAKING_POOL_MODULE_NAME;
use crate::messages::Transaction;
use crate::messages::VerifiedTransaction;
use crate::messages_checkpoint::CheckpointTimestamp;
use crate::multisig::MultiSigPublicKey;
use crate::object::{Object, Owner};
use crate::parse_sui_struct_tag;
use crate::signature::GenericSignature;
use crate::sui_serde::HexAccountAddress;
use crate::sui_serde::Readable;
use crate::MOVE_STDLIB_ADDRESS;
use crate::SUI_CLOCK_OBJECT_ID;
use crate::SUI_FRAMEWORK_ADDRESS;
use crate::SUI_SYSTEM_ADDRESS;
use anyhow::anyhow;
use fastcrypto::encoding::decode_bytes_hex;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::AllowedRng;
use move_binary_format::binary_views::BinaryIndexedView;
use move_binary_format::file_format::SignatureToken;
use move_bytecode_utils::resolve_struct;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::ModuleId;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use rand::Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::HashingIntentScope;
use std::cmp::max;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;

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
#[cfg_attr(feature = "fuzzing", derive(proptest_derive::Arbitrary))]
pub struct SequenceNumber(u64);

impl SequenceNumber {
    pub fn one_before(&self) -> Option<SequenceNumber> {
        if self.0 == 0 {
            None
        } else {
            Some(SequenceNumber(self.0 - 1))
        }
    }
}

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
    #[serde_as(as = "Readable<HexAccountAddress, _>")]
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

/// Wrapper around StructTag with a space-efficient representation for common types like coins
/// The StructTag for a gas coin is 84 bytes, so using 1 byte instead is a win.
/// The inner representation is private to prevent incorrectly constructing an `Other` instead of
/// one of the specialized variants, e.g. `Other(GasCoin::type_())` instead of `GasCoin`
#[derive(Eq, PartialEq, PartialOrd, Ord, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObjectType(MoveObjectType_);

/// Even though it is declared public, it is the "private", internal representation for
/// `MoveObjectType`
#[derive(Eq, PartialEq, PartialOrd, Ord, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum MoveObjectType_ {
    /// A type that is not `0x2::coin::Coin<T>`
    Other(StructTag),
    /// A SUI coin (i.e., `0x2::coin::Coin<0x2::sui::SUI>`)
    GasCoin,
    /// A record of a staked SUI coin (i.e., `0x3::staking_pool::StakedSui`)
    StakedSui,
    /// A non-SUI coin type (i.e., `0x2::coin::Coin<T> where T != 0x2::sui::SUI`)
    Coin(TypeTag),
    // NOTE: if adding a new type here, and there are existing on-chain objects of that
    // type with Other(_), that is ok, but you must hand-roll PartialEq/Eq/Ord/maybe Hash
    // to make sure the new type and Other(_) are interpreted consistently.
}

impl MoveObjectType {
    pub fn gas_coin() -> Self {
        Self(MoveObjectType_::GasCoin)
    }

    pub fn staked_sui() -> Self {
        Self(MoveObjectType_::StakedSui)
    }

    pub fn address(&self) -> AccountAddress {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::Coin(_) => SUI_FRAMEWORK_ADDRESS,
            MoveObjectType_::StakedSui => SUI_SYSTEM_ADDRESS,
            MoveObjectType_::Other(s) => s.address,
        }
    }

    pub fn module(&self) -> &IdentStr {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::Coin(_) => COIN_MODULE_NAME,
            MoveObjectType_::StakedSui => STAKING_POOL_MODULE_NAME,
            MoveObjectType_::Other(s) => &s.module,
        }
    }

    pub fn name(&self) -> &IdentStr {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::Coin(_) => COIN_STRUCT_NAME,
            MoveObjectType_::StakedSui => STAKED_SUI_STRUCT_NAME,
            MoveObjectType_::Other(s) => &s.name,
        }
    }

    pub fn type_params(&self) -> Vec<TypeTag> {
        match &self.0 {
            MoveObjectType_::GasCoin => vec![GAS::type_tag()],
            MoveObjectType_::StakedSui => vec![],
            MoveObjectType_::Coin(inner) => vec![inner.clone()],
            MoveObjectType_::Other(s) => s.type_params.clone(),
        }
    }

    pub fn into_type_params(self) -> Vec<TypeTag> {
        match self.0 {
            MoveObjectType_::GasCoin => vec![GAS::type_tag()],
            MoveObjectType_::StakedSui => vec![],
            MoveObjectType_::Coin(inner) => vec![inner],
            MoveObjectType_::Other(s) => s.type_params,
        }
    }

    pub fn coin_type_maybe(&self) -> Option<TypeTag> {
        match &self.0 {
            MoveObjectType_::GasCoin => Some(GAS::type_tag()),
            MoveObjectType_::Coin(inner) => Some(inner.clone()),
            MoveObjectType_::StakedSui => None,
            MoveObjectType_::Other(_) => None,
        }
    }

    pub fn module_id(&self) -> ModuleId {
        ModuleId::new(self.address(), self.module().to_owned())
    }

    pub fn size_for_gas_metering(&self) -> usize {
        // unwraps safe because a `StructTag` cannot fail to serialize
        match &self.0 {
            MoveObjectType_::GasCoin => 1,
            MoveObjectType_::StakedSui => 1,
            MoveObjectType_::Coin(inner) => bcs::serialized_size(inner).unwrap() + 1,
            MoveObjectType_::Other(s) => bcs::serialized_size(s).unwrap() + 1,
        }
    }

    /// Return true if `self` is `0x2::coin::Coin<T>` for some T (note: T can be SUI)
    pub fn is_coin(&self) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::Coin(_) => true,
            MoveObjectType_::StakedSui | MoveObjectType_::Other(_) => false,
        }
    }

    /// Return true if `self` is 0x2::coin::Coin<0x2::sui::SUI>
    pub fn is_gas_coin(&self) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin => true,
            MoveObjectType_::StakedSui | MoveObjectType_::Coin(_) | MoveObjectType_::Other(_) => {
                false
            }
        }
    }

    /// Return true if `self` is `0x2::coin::Coin<t>`
    pub fn is_coin_t(&self, t: &TypeTag) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin => GAS::is_gas_type(t),
            MoveObjectType_::Coin(c) => t == c,
            MoveObjectType_::StakedSui | MoveObjectType_::Other(_) => false,
        }
    }

    pub fn is_staked_sui(&self) -> bool {
        match &self.0 {
            MoveObjectType_::StakedSui => true,
            MoveObjectType_::GasCoin | MoveObjectType_::Coin(_) | MoveObjectType_::Other(_) => {
                false
            }
        }
    }

    pub fn is_coin_metadata(&self) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::StakedSui | MoveObjectType_::Coin(_) => {
                false
            }
            MoveObjectType_::Other(s) => CoinMetadata::is_coin_metadata(s),
        }
    }

    pub fn is_dynamic_field(&self) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::StakedSui | MoveObjectType_::Coin(_) => {
                false
            }
            MoveObjectType_::Other(s) => DynamicFieldInfo::is_dynamic_field(s),
        }
    }

    pub fn try_extract_field_name(&self, type_: &DynamicFieldType) -> SuiResult<TypeTag> {
        match &self.0 {
            MoveObjectType_::GasCoin | MoveObjectType_::StakedSui | MoveObjectType_::Coin(_) => {
                Err(SuiError::ObjectDeserializationError {
                    error: "Error extracting dynamic object name from Coin object".to_string(),
                })
            }
            MoveObjectType_::Other(s) => DynamicFieldInfo::try_extract_field_name(s, type_),
        }
    }

    pub fn is(&self, s: &StructTag) -> bool {
        match &self.0 {
            MoveObjectType_::GasCoin => GasCoin::is_gas_coin(s),
            MoveObjectType_::StakedSui => StakedSui::is_staked_sui(s),
            MoveObjectType_::Coin(inner) => {
                Coin::is_coin(s) && s.type_params.len() == 1 && inner == &s.type_params[0]
            }
            MoveObjectType_::Other(o) => s == o,
        }
    }
}

impl From<StructTag> for MoveObjectType {
    fn from(mut s: StructTag) -> Self {
        Self(if GasCoin::is_gas_coin(&s) {
            MoveObjectType_::GasCoin
        } else if Coin::is_coin(&s) {
            // unwrap safe because a coin has exactly one type parameter
            MoveObjectType_::Coin(s.type_params.pop().unwrap())
        } else if StakedSui::is_staked_sui(&s) {
            MoveObjectType_::StakedSui
        } else {
            MoveObjectType_::Other(s)
        })
    }
}

impl From<MoveObjectType> for StructTag {
    fn from(t: MoveObjectType) -> Self {
        match t.0 {
            MoveObjectType_::GasCoin => GasCoin::type_(),
            MoveObjectType_::StakedSui => StakedSui::type_(),
            MoveObjectType_::Coin(inner) => Coin::type_(inner),
            MoveObjectType_::Other(s) => s,
        }
    }
}

impl From<MoveObjectType> for TypeTag {
    fn from(o: MoveObjectType) -> TypeTag {
        let s: StructTag = o.into();
        TypeTag::Struct(Box::new(s))
    }
}

/// Type of a Sui object
#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum ObjectType {
    /// Move package containing one or more bytecode modules
    Package,
    /// A Move struct of the given type
    Struct(MoveObjectType),
}

impl From<&Object> for ObjectType {
    fn from(o: &Object) -> Self {
        o.data
            .type_()
            .map(|t| ObjectType::Struct(t.clone()))
            .unwrap_or(ObjectType::Package)
    }
}

impl TryFrom<ObjectType> for StructTag {
    type Error = anyhow::Error;

    fn try_from(o: ObjectType) -> Result<Self, anyhow::Error> {
        match o {
            ObjectType::Package => Err(anyhow!("Cannot create StructTag from Package")),
            ObjectType::Struct(move_object_type) => Ok(move_object_type.into()),
        }
    }
}

impl FromStr for ObjectType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.to_lowercase() == PACKAGE {
            Ok(ObjectType::Package)
        } else {
            let tag = parse_sui_struct_tag(s)?;
            Ok(ObjectType::Struct(MoveObjectType::from(tag)))
        }
    }
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
        Self {
            object_id,
            version,
            digest,
            type_: o.into(),
            owner: o.owner,
            previous_transaction: o.previous_transaction,
        }
    }
}
const PACKAGE: &str = "package";
impl ObjectType {
    pub fn is_gas_coin(&self) -> bool {
        matches!(self, ObjectType::Struct(s) if s.is_gas_coin())
    }

    pub fn is_coin(&self) -> bool {
        matches!(self, ObjectType::Struct(s) if s.is_coin())
    }

    /// Return true if `self` is `0x2::coin::Coin<t>`
    pub fn is_coin_t(&self, t: &TypeTag) -> bool {
        matches!(self, ObjectType::Struct(s) if s.is_coin_t(t))
    }

    pub fn is_package(&self) -> bool {
        matches!(self, ObjectType::Package)
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
#[cfg_attr(feature = "fuzzing", derive(proptest_derive::Arbitrary))]
pub struct SuiAddress(
    #[schemars(with = "Hex")]
    #[serde_as(as = "Readable<Hex, _>")]
    [u8; SUI_ADDRESS_LENGTH],
);

impl SuiAddress {
    pub const ZERO: Self = Self([0u8; SUI_ADDRESS_LENGTH]);

    /// Convert the address to a byte buffer.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    #[cfg(feature = "test-utils")]
    /// Return a random SuiAddress.
    pub fn random_for_testing_only() -> Self {
        AccountAddress::random().into()
    }

    /// Serialize an `Option<SuiAddress>` in Hex.
    pub fn optional_address_as_hex<S>(
        key: &Option<SuiAddress>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&key.map(Hex::encode).unwrap_or_default())
    }

    /// Deserialize into an `Option<SuiAddress>`.
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

    /// Return the underlying byte array of a SuiAddress.
    pub fn to_inner(self) -> [u8; SUI_ADDRESS_LENGTH] {
        self.0
    }

    /// Parse a SuiAddress from a byte array or buffer.
    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Self, SuiError> {
        <[u8; SUI_ADDRESS_LENGTH]>::try_from(bytes.as_ref())
            .map_err(|_| SuiError::InvalidAddress)
            .map(SuiAddress)
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

impl TryFrom<&[u8]> for SuiAddress {
    type Error = SuiError;

    /// Tries to convert the provided byte array into a SuiAddress.
    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<Vec<u8>> for SuiAddress {
    type Error = SuiError;

    /// Tries to convert the provided byte buffer into a SuiAddress.
    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Self::from_bytes(bytes)
    }
}

impl AsRef<[u8]> for SuiAddress {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl FromStr for SuiAddress {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        decode_bytes_hex(s).map_err(|e| anyhow!(e))
    }
}

impl<T: SuiPublicKey> From<&T> for SuiAddress {
    fn from(pk: &T) -> Self {
        let mut hasher = DefaultHash::default();
        hasher.update([T::SIGNATURE_SCHEME.flag()]);
        hasher.update(pk);
        let g_arr = hasher.finalize();
        SuiAddress(g_arr.digest)
    }
}

impl From<&PublicKey> for SuiAddress {
    fn from(pk: &PublicKey) -> Self {
        let mut hasher = DefaultHash::default();
        hasher.update([pk.flag()]);
        hasher.update(pk);
        let g_arr = hasher.finalize();
        SuiAddress(g_arr.digest)
    }
}

impl From<MultiSigPublicKey> for SuiAddress {
    /// Derive a SuiAddress from [struct MultiSigPublicKey]. A MultiSig address
    /// is defined as the 32-byte Blake2b hash of serializing the flag, the
    /// threshold, concatenation of each participating flag, public keys and
    /// its weight. `flag_MultiSig || threshold || flag_1 || pk_1 || weight_1
    /// || ... || flag_n || pk_n || weight_n`.
    fn from(multisig_pk: MultiSigPublicKey) -> Self {
        let mut hasher = DefaultHash::default();
        hasher.update([SignatureScheme::MultiSig.flag()]);
        hasher.update(multisig_pk.threshold().to_le_bytes());
        multisig_pk.pubkeys().iter().for_each(|(pk, w)| {
            hasher.update([pk.flag()]);
            hasher.update(pk.as_ref());
            hasher.update(w.to_le_bytes());
        });
        SuiAddress(hasher.finalize().digest)
    }
}

impl TryFrom<&GenericSignature> for SuiAddress {
    type Error = SuiError;
    /// Derive a SuiAddress from a serialized signature in Sui [GenericSignature].
    fn try_from(sig: &GenericSignature) -> SuiResult<Self> {
        Ok(match sig {
            GenericSignature::Signature(sig) => {
                let scheme = sig.scheme();
                let pub_key_bytes = sig.public_key_bytes();
                let pub_key = PublicKey::try_from_bytes(scheme, pub_key_bytes).map_err(|_| {
                    SuiError::InvalidSignature {
                        error: "Cannot parse pubkey".to_string(),
                    }
                })?;
                SuiAddress::from(&pub_key)
            }
            GenericSignature::MultiSig(ms) => ms.multisig_pk.clone().into(),
        })
    }
}

impl fmt::Display for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", Hex::encode(self.0))
    }
}

impl fmt::Debug for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "0x{}", Hex::encode(self.0))
    }
}

#[cfg(feature = "test-utils")]
/// Generate a fake SuiAddress with repeated one byte.
pub fn dbg_addr(name: u8) -> SuiAddress {
    let addr = [name; SUI_ADDRESS_LENGTH];
    SuiAddress(addr)
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

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct ExecutionData {
    pub transaction: Transaction,
    pub effects: TransactionEffects,
}

impl ExecutionData {
    pub fn new(transaction: Transaction, effects: TransactionEffects) -> ExecutionData {
        debug_assert_eq!(transaction.digest(), effects.transaction_digest());
        Self {
            transaction,
            effects,
        }
    }

    pub fn digests(&self) -> ExecutionDigests {
        self.effects.execution_digests()
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct VerifiedExecutionData {
    pub transaction: VerifiedTransaction,
    pub effects: TransactionEffects,
}

impl VerifiedExecutionData {
    pub fn new(transaction: VerifiedTransaction, effects: TransactionEffects) -> Self {
        debug_assert_eq!(transaction.digest(), effects.transaction_digest());
        Self {
            transaction,
            effects,
        }
    }

    pub fn new_unchecked(data: ExecutionData) -> Self {
        Self {
            transaction: VerifiedTransaction::new_unchecked(data.transaction),
            effects: data.effects,
        }
    }

    pub fn into_inner(self) -> ExecutionData {
        ExecutionData {
            transaction: self.transaction.into_inner(),
            effects: self.effects,
        }
    }

    pub fn digests(&self) -> ExecutionDigests {
        self.effects.execution_digests()
    }
}

pub const STD_OPTION_MODULE_NAME: &IdentStr = ident_str!("option");
pub const STD_OPTION_STRUCT_NAME: &IdentStr = ident_str!("Option");
pub const RESOLVED_STD_OPTION: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_OPTION_MODULE_NAME,
    STD_OPTION_STRUCT_NAME,
);

pub const STD_ASCII_MODULE_NAME: &IdentStr = ident_str!("ascii");
pub const STD_ASCII_STRUCT_NAME: &IdentStr = ident_str!("String");
pub const RESOLVED_ASCII_STR: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_ASCII_MODULE_NAME,
    STD_ASCII_STRUCT_NAME,
);

pub const STD_UTF8_MODULE_NAME: &IdentStr = ident_str!("string");
pub const STD_UTF8_STRUCT_NAME: &IdentStr = ident_str!("String");
pub const RESOLVED_UTF8_STR: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_UTF8_MODULE_NAME,
    STD_UTF8_STRUCT_NAME,
);

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
    /// Timestamp that the epoch started at
    epoch_timestamp_ms: CheckpointTimestamp,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TxContextKind {
    // No TxContext
    None,
    // &mut TxContext
    Mutable,
    // &TxContext
    Immutable,
}

impl TxContext {
    pub fn new(sender: &SuiAddress, digest: &TransactionDigest, epoch_data: &EpochData) -> Self {
        Self::new_from_components(
            sender,
            digest,
            &epoch_data.epoch_id(),
            epoch_data.epoch_start_timestamp(),
        )
    }

    pub fn new_from_components(
        sender: &SuiAddress,
        digest: &TransactionDigest,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
    ) -> Self {
        Self {
            sender: AccountAddress::new(sender.0),
            digest: digest.into_inner().to_vec(),
            epoch: *epoch_id,
            epoch_timestamp_ms,
            ids_created: 0,
        }
    }

    /// Returns whether the type signature is &mut TxContext, &TxContext, or none of the above.
    pub fn kind(view: &BinaryIndexedView<'_>, s: &SignatureToken) -> TxContextKind {
        use SignatureToken as S;
        let (kind, s) = match s {
            S::MutableReference(s) => (TxContextKind::Mutable, s),
            S::Reference(s) => (TxContextKind::Immutable, s),
            _ => return TxContextKind::None,
        };

        let S::Struct(idx) = &**s else {
            return TxContextKind::None;
        };

        let (module_addr, module_name, struct_name) = resolve_struct(view, *idx);
        let is_tx_context_type = module_name == TX_CONTEXT_MODULE_NAME
            && module_addr == &SUI_FRAMEWORK_ADDRESS
            && struct_name == TX_CONTEXT_STRUCT_NAME;

        if is_tx_context_type {
            kind
        } else {
            TxContextKind::None
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    /// Derive a globally unique object ID by hashing self.digest | self.ids_created
    pub fn fresh_id(&mut self) -> ObjectID {
        let id = ObjectID::derive_id(self.digest(), self.ids_created);

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
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::InvariantViolation,
                "Immutable fields for TxContext changed",
            ));
        }
        self.ids_created = other.ids_created;
        Ok(())
    }

    #[cfg(feature = "test-utils")]
    // Generate a random TxContext for testing.
    pub fn random_for_testing_only() -> Self {
        Self::new(
            &SuiAddress::random_for_testing_only(),
            &TransactionDigest::random(),
            &EpochData::new_test(),
        )
    }

    #[cfg(feature = "test-utils")]
    /// Generate a TxContext for testing with a specific sender.
    pub fn with_sender_for_testing_only(sender: &SuiAddress) -> Self {
        Self::new(sender, &TransactionDigest::random(), &EpochData::new_test())
    }
}

// TODO: rename to version
impl SequenceNumber {
    pub const MIN: SequenceNumber = SequenceNumber(u64::MIN);
    pub const MAX: SequenceNumber = SequenceNumber(0x7fff_ffff_ffff_ffff);

    pub const fn new() -> Self {
        SequenceNumber(0)
    }

    pub const fn value(&self) -> u64 {
        self.0
    }

    pub const fn from_u64(u: u64) -> Self {
        SequenceNumber(u)
    }

    pub fn increment(&mut self) {
        assert_ne!(self.0, u64::MAX);
        self.0 += 1;
    }

    pub fn increment_to(&mut self, next: SequenceNumber) {
        debug_assert!(*self < next, "Not an increment: {} to {}", self, next);
        *self = next;
    }

    pub fn decrement(&mut self) {
        assert_ne!(self.0, 0);
        self.0 -= 1;
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

impl ObjectID {
    /// The number of bytes in an address.
    pub const LENGTH: usize = AccountAddress::LENGTH;
    /// Hex address: 0x0
    pub const ZERO: Self = Self::new([0u8; Self::LENGTH]);
    pub const MAX: Self = Self::new([0xff; Self::LENGTH]);
    /// Create a new ObjectID
    pub const fn new(obj_id: [u8; Self::LENGTH]) -> Self {
        Self(AccountAddress::new(obj_id))
    }

    /// Const fn variant of `<ObjectID as From<AccountAddress>>::from`
    pub const fn from_address(addr: AccountAddress) -> Self {
        Self(addr)
    }

    /// Return a random ObjectID.
    pub fn random() -> Self {
        Self::from(AccountAddress::random())
    }

    /// Return a random ObjectID from a given RNG.
    pub fn random_from_rng<R>(rng: &mut R) -> Self
    where
        R: AllowedRng,
    {
        let buf: [u8; Self::LENGTH] = rng.gen();
        ObjectID::new(buf)
    }

    /// Return the underlying bytes buffer of the ObjectID.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Parse the ObjectID from byte array or buffer.
    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Self, ObjectIDParseError> {
        <[u8; Self::LENGTH]>::try_from(bytes.as_ref())
            .map_err(|_| ObjectIDParseError::TryFromSliceError)
            .map(ObjectID::new)
    }

    /// Return the underlying bytes array of the ObjectID.
    pub fn into_bytes(self) -> [u8; Self::LENGTH] {
        self.0.into_bytes()
    }

    /// Make an ObjectID with padding 0s before the single byte.
    pub const fn from_single_byte(byte: u8) -> ObjectID {
        let mut bytes = [0u8; Self::LENGTH];
        bytes[Self::LENGTH - 1] = byte;
        ObjectID::new(bytes)
    }

    /// Convert from hex string to ObjectID where the string is prefixed with 0x
    /// Padding 0s if the string is too short.
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

    /// Create an ObjectID from `TransactionDigest` and `creation_num`.
    /// Caller is responsible for ensuring that `creation_num` is fresh
    pub fn derive_id(digest: TransactionDigest, creation_num: u64) -> Self {
        let mut hasher = DefaultHash::default();
        hasher.update([HashingIntentScope::RegularObjectId as u8]);
        hasher.update(digest);
        hasher.update(creation_num.to_le_bytes());
        let hash = hasher.finalize();

        // truncate into an ObjectID.
        // OK to access slice because digest should never be shorter than ObjectID::LENGTH.
        ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap()
    }

    /// Incremenent the ObjectID by usize IDs, assuming the ObjectID hex is a number represented as an array of bytes
    pub fn advance(&self, step: usize) -> Result<ObjectID, anyhow::Error> {
        let mut curr_vec = self.to_vec();
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
        ObjectID::try_from(curr_vec).map_err(|w| w.into())
    }

    /// Increment the ObjectID by one, assuming the ObjectID hex is a number represented as an array of bytes
    pub fn next_increment(&self) -> Result<ObjectID, anyhow::Error> {
        let mut prev_val = self.to_vec();
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
        ObjectID::try_from(prev_val.clone()).map_err(|w| w.into())
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

    /// Return the full hex string with 0x prefix without removing trailing 0s. Prefer this
    /// over [fn to_hex_literal] if the string needs to be fully preserved.
    pub fn to_hex_uncompressed(&self) -> String {
        format!("{self}")
    }

    pub fn is_clock(&self) -> bool {
        *self == SUI_CLOCK_OBJECT_ID
    }
}

impl From<SuiAddress> for ObjectID {
    fn from(address: SuiAddress) -> ObjectID {
        let tmp: AccountAddress = address.into();
        tmp.into()
    }
}

impl From<AccountAddress> for ObjectID {
    fn from(address: AccountAddress) -> Self {
        Self(address)
    }
}

impl fmt::Display for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "0x{}", Hex::encode(self.0))
    }
}

impl fmt::Debug for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "0x{}", Hex::encode(self.0))
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

impl FromStr for ObjectID {
    type Err = ObjectIDParseError;

    /// Parse ObjectID from hex string with or without 0x prefix, pad with 0s if needed.
    fn from_str(s: &str) -> Result<Self, ObjectIDParseError> {
        decode_bytes_hex(s).or_else(|_| Self::from_hex_literal(s))
    }
}

impl std::ops::Deref for ObjectID {
    type Target = AccountAddress;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "test-utils")]
/// Generate a fake ObjectID with repeated one byte.
pub fn dbg_object_id(name: u8) -> ObjectID {
    ObjectID::new([name; ObjectID::LENGTH])
}

#[derive(PartialEq, Eq, Clone, Debug, thiserror::Error)]
pub enum ObjectIDParseError {
    #[error("ObjectID hex literal must start with 0x")]
    HexLiteralPrefixMissing,

    #[error("Could not convert from bytes slice")]
    TryFromSliceError,
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

impl fmt::Display for MoveObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let s: StructTag = self.clone().into();
        write!(f, "{}", s)
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Package => write!(f, "{}", PACKAGE),
            ObjectType::Struct(t) => write!(f, "{}", t),
        }
    }
}
