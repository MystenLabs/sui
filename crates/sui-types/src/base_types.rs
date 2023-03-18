// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::coin::Coin;
use crate::coin::CoinMetadata;
use crate::coin::LockedCoin;
use crate::coin::COIN_MODULE_NAME;
use crate::coin::COIN_STRUCT_NAME;
pub use crate::committee::EpochId;
use crate::crypto::{
    AuthorityPublicKeyBytes, DefaultHash, KeypairTraits, PublicKey, SignatureScheme, SuiPublicKey,
    SuiSignature,
};
pub use crate::digests::{ObjectDigest, TransactionDigest, TransactionEffectsDigest};
use crate::dynamic_field::DynamicFieldInfo;
use crate::dynamic_field::DynamicFieldType;
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
use crate::messages::TransactionEffects;
use crate::messages::TransactionEffectsAPI;
use crate::messages::VerifiedTransaction;
use crate::messages_checkpoint::CheckpointTimestamp;
use crate::multisig::MultiSigPublicKey;
use crate::object::{Object, Owner};
use crate::parse_sui_struct_tag;
use crate::signature::GenericSignature;
use crate::sui_serde::HexAccountAddress;
use crate::sui_serde::Readable;
use crate::SUI_FRAMEWORK_ADDRESS;
use anyhow::anyhow;
use fastcrypto::encoding::decode_bytes_hex;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;
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
/// The StructTag for a gas coin is 84 bytes, so using 1 byte instead is a win
#[derive(Eq, PartialEq, PartialOrd, Ord, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum MoveObjectType {
    /// A type that is not 0x2::coin::Coin<T>
    Other(StructTag),
    /// A SUI coin (i.e., 0x2::coin::Coin<0x2::sui::SUI>)
    GasCoin,
    /// A record of a staked SUI coin (i.e., 0x2::staking_pool::StakedSui)
    StakedSui,
    /// A non-SUI coin type (i.e., 0x2::coin::Coin<T> where T != 0x2::sui::SUI)
    Coin(TypeTag),
    // NOTE: if adding a new type here, and there are existing on-chain objects of that
    // type with Other(_), that is ok, but you must hand-roll PartialEq/Eq/Ord/maybe Hash
    // to make sure the new type and Other(_) are interpreted consistently.
}

impl MoveObjectType {
    pub fn address(&self) -> AccountAddress {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::StakedSui | MoveObjectType::Coin(_) => {
                SUI_FRAMEWORK_ADDRESS
            }
            MoveObjectType::Other(s) => s.address,
        }
    }

    pub fn module(&self) -> &IdentStr {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::Coin(_) => COIN_MODULE_NAME,
            MoveObjectType::StakedSui => STAKING_POOL_MODULE_NAME,
            MoveObjectType::Other(s) => &s.module,
        }
    }

    pub fn name(&self) -> &IdentStr {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::Coin(_) => COIN_STRUCT_NAME,
            MoveObjectType::StakedSui => STAKED_SUI_STRUCT_NAME,
            MoveObjectType::Other(s) => &s.name,
        }
    }

    pub fn type_params(&self) -> Vec<TypeTag> {
        match self {
            MoveObjectType::GasCoin => vec![GAS::type_tag()],
            MoveObjectType::StakedSui => vec![],
            MoveObjectType::Coin(inner) => vec![inner.clone()],
            MoveObjectType::Other(s) => s.type_params.clone(),
        }
    }

    pub fn into_type_params(self) -> Vec<TypeTag> {
        match self {
            MoveObjectType::GasCoin => vec![GAS::type_tag()],
            MoveObjectType::StakedSui => vec![],
            MoveObjectType::Coin(inner) => vec![inner],
            MoveObjectType::Other(s) => s.type_params,
        }
    }

    pub fn module_id(&self) -> ModuleId {
        ModuleId::new(self.address(), self.module().to_owned())
    }

    pub fn size_for_gas_metering(&self) -> usize {
        // unwraps safe because a `StructTag` cannot fail to serialize
        match self {
            MoveObjectType::GasCoin => 1,
            MoveObjectType::StakedSui => 1,
            MoveObjectType::Coin(inner) => bcs::serialized_size(inner).unwrap() + 1,
            MoveObjectType::Other(s) => bcs::serialized_size(s).unwrap() + 1,
        }
    }

    /// Return true if `self` is 0x2::coin::Coin<T> for some T (note: T can be SUI)
    pub fn is_coin(&self) -> bool {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::Coin(_) => true,
            MoveObjectType::StakedSui | MoveObjectType::Other(_) => false,
        }
    }

    /// Return true if `self` is 0x2::coin::Coin<0x2::sui::SUI>
    pub fn is_gas_coin(&self) -> bool {
        match self {
            MoveObjectType::GasCoin => true,
            MoveObjectType::StakedSui | MoveObjectType::Coin(_) | MoveObjectType::Other(_) => false,
        }
    }

    pub fn is_staked_sui(&self) -> bool {
        match self {
            MoveObjectType::StakedSui => true,
            MoveObjectType::GasCoin | MoveObjectType::Coin(_) | MoveObjectType::Other(_) => false,
        }
    }

    pub fn is_locked_coin(&self) -> bool {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::StakedSui | MoveObjectType::Coin(_) => false,
            MoveObjectType::Other(s) => LockedCoin::is_locked_coin(s),
        }
    }

    pub fn is_coin_metadata(&self) -> bool {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::StakedSui | MoveObjectType::Coin(_) => false,
            MoveObjectType::Other(s) => CoinMetadata::is_coin_metadata(s),
        }
    }

    pub fn is_dynamic_field(&self) -> bool {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::StakedSui | MoveObjectType::Coin(_) => false,
            MoveObjectType::Other(s) => DynamicFieldInfo::is_dynamic_field(s),
        }
    }

    pub fn try_extract_field_name(&self, type_: &DynamicFieldType) -> SuiResult<TypeTag> {
        match self {
            MoveObjectType::GasCoin | MoveObjectType::StakedSui | MoveObjectType::Coin(_) => {
                Err(SuiError::ObjectDeserializationError {
                    error: "Error extracting dynamic object name from Coin object".to_string(),
                })
            }
            MoveObjectType::Other(s) => DynamicFieldInfo::try_extract_field_name(s, type_),
        }
    }

    pub fn is(&self, s: &StructTag) -> bool {
        match self {
            MoveObjectType::GasCoin => GasCoin::is_gas_coin(s),
            MoveObjectType::StakedSui => StakedSui::is_staked_sui(s),
            MoveObjectType::Coin(inner) => {
                Coin::is_coin(s) && s.type_params.len() == 1 && inner == &s.type_params[0]
            }
            MoveObjectType::Other(o) => s == o,
        }
    }

    pub fn matches_type_fuzzy_generics(&self, other_type: &Self) -> bool {
        self.address() == other_type.address()
            && self.module() == other_type.module()
            && self.name() == other_type.name()
            // MUSTFIX: is_empty() looks like a bug here. I think the intention is to support "fuzzy matching" where `get_move_objects`
            // leaves type_params unspecified, but I don't actually see any call sites taking advantage of this
            && (self.type_params().is_empty() || self.type_params() == other_type.type_params())
    }
}

impl From<StructTag> for MoveObjectType {
    fn from(mut s: StructTag) -> Self {
        if GasCoin::is_gas_coin(&s) {
            MoveObjectType::GasCoin
        } else if Coin::is_coin(&s) {
            // unwrap safe because a coin has exactly one type parameter
            MoveObjectType::Coin(s.type_params.pop().unwrap())
        } else if StakedSui::is_staked_sui(&s) {
            MoveObjectType::StakedSui
        } else {
            MoveObjectType::Other(s)
        }
    }
}

impl From<MoveObjectType> for StructTag {
    fn from(o: MoveObjectType) -> Self {
        match o {
            MoveObjectType::GasCoin => GasCoin::type_(),
            MoveObjectType::StakedSui => StakedSui::type_(),
            MoveObjectType::Coin(inner) => Coin::type_(inner),
            MoveObjectType::Other(s) => s,
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
        match self {
            ObjectType::Struct(s) => s.is_gas_coin(),
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

/// A MultiSig address is the first 20 bytes of the hash of
/// `flag_MultiSig || threshold || flag_1 || pk_1 || weight_1 || ... || flag_n || pk_n || weight_n`
/// of all participating public keys and its weight.
impl From<MultiSigPublicKey> for SuiAddress {
    fn from(multisig_pk: MultiSigPublicKey) -> Self {
        let mut hasher = DefaultHash::default();
        hasher.update([SignatureScheme::MultiSig.flag()]);
        hasher.update(multisig_pk.threshold().to_le_bytes());
        multisig_pk.pubkeys().iter().for_each(|(pk, w)| {
            hasher.update([pk.flag()]);
            hasher.update(pk.as_ref());
            hasher.update(w.to_le_bytes());
        });
        let g_arr = hasher.finalize();
        SuiAddress(g_arr.digest)
    }
}

impl TryFrom<&GenericSignature> for SuiAddress {
    type Error = SuiError;
    fn try_from(sig: &GenericSignature) -> SuiResult<Self> {
        Ok(match sig {
            GenericSignature::Signature(sig) => {
                let scheme = sig.scheme();
                let pub_key_bytes = sig.public_key_bytes();
                let pub_key = PublicKey::try_from_bytes(scheme, pub_key_bytes).map_err(|e| {
                    SuiError::InvalidSignature {
                        error: e.to_string(),
                    }
                })?;
                SuiAddress::from(&pub_key)
            }
            GenericSignature::MultiSig(ms) => ms.multisig_pk.clone().into(),
        })
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
    /// Timestamp that the epoch started at
    epoch_timestamp_ms: CheckpointTimestamp,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl TxContext {
    pub fn new(sender: &SuiAddress, digest: &TransactionDigest, epoch_data: &EpochData) -> Self {
        Self {
            sender: AccountAddress::new(sender.0),
            digest: digest.into_inner().to_vec(),
            epoch: epoch_data.epoch_id(),
            epoch_timestamp_ms: epoch_data.epoch_start_timestamp(),
            ids_created: 0,
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

    // for testing
    pub fn random_for_testing_only() -> Self {
        Self::new(
            &SuiAddress::random_for_testing_only(),
            &TransactionDigest::random(),
            &EpochData::new_test(),
        )
    }

    // for testing
    pub fn with_sender_for_testing_only(sender: &SuiAddress) -> Self {
        Self::new(sender, &TransactionDigest::random(), &EpochData::new_test())
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

    /// Returns the full hex string with 0x prefix without removing trailing 0s. Prefer this
    /// over [fn to_hex_literal] if the string needs to be fully preserved.
    pub fn to_hex_uncompressed(&self) -> String {
        format!("0x{:x}", self)
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
