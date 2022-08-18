// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This file contain response types used by the GatewayAPI, most of the types mirrors it's internal type counterparts.
/// These mirrored types allow us to optimise the JSON serde without impacting the internal types, which are optimise for storage.
///
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use colored::Colorize;
use either::Either;
use itertools::Itertools;
use move_binary_format::file_format::{Ability, AbilitySet, StructTypeParameter, Visibility};
use move_binary_format::normalized::{
    Field as NormalizedField, Function as SuiNormalizedFunction, Module as NormalizedModule,
    Struct as NormalizedStruct, Type as NormalizedType,
};
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::parser::{parse_struct_tag, parse_type_tag};
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use schemars::JsonSchema;
use serde::ser::Error;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_with::serde_as;
use tracing::warn;

use sui_json::SuiJsonValue;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    TransactionEffectsDigest,
};
use sui_types::committee::EpochId;
use sui_types::crypto::{AuthorityStrongQuorumSignInfo, SignableBytes, Signature};
use sui_types::error::SuiError;
use sui_types::event::EventType;
use sui_types::event::{Event, TransferType};
use sui_types::event_filter::EventFilter;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, CertifiedTransaction, CertifiedTransactionEffects, ExecuteTransactionResponse,
    ExecutionStatus, InputObjectKind, MoveModulePublish, ObjectArg, SenderSignedData,
    SingleTransactionKind, TransactionData, TransactionEffects, TransactionKind,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::move_package::disassemble_modules;
use sui_types::object::{Data, MoveObject, Object, ObjectFormatOptions, ObjectRead, Owner};
use sui_types::sui_serde::{Base64, Encoding};

#[cfg(test)]
#[path = "unit_tests/rpc_types_tests.rs"]
mod rpc_types_tests;

pub type GatewayTxSeqNumber = u64;
pub type SuiMoveTypeParameterIndex = u16;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum SuiMoveAbility {
    Copy,
    Drop,
    Store,
    Key,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveAbilitySet {
    pub abilities: Vec<SuiMoveAbility>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum SuiMoveVisibility {
    Private,
    Public,
    Friend,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveStructTypeParameter {
    pub constraints: SuiMoveAbilitySet,
    pub is_phantom: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveNormalizedField {
    pub name: String,
    pub type_: SuiMoveNormalizedType,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveNormalizedStruct {
    pub abilities: SuiMoveAbilitySet,
    pub type_parameters: Vec<SuiMoveStructTypeParameter>,
    pub fields: Vec<SuiMoveNormalizedField>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum SuiMoveNormalizedType {
    Bool,
    U8,
    U64,
    U128,
    Address,
    Signer,
    Struct {
        address: String,
        module: String,
        name: String,
        type_arguments: Vec<SuiMoveNormalizedType>,
    },
    Vector(Box<SuiMoveNormalizedType>),
    TypeParameter(SuiMoveTypeParameterIndex),
    Reference(Box<SuiMoveNormalizedType>),
    MutableReference(Box<SuiMoveNormalizedType>),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveNormalizedFunction {
    pub visibility: SuiMoveVisibility,
    pub is_entry: bool,
    pub type_parameters: Vec<SuiMoveAbilitySet>,
    pub parameters: Vec<SuiMoveNormalizedType>,
    pub return_: Vec<SuiMoveNormalizedType>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveModuleId {
    address: String,
    name: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiMoveNormalizedModule {
    pub file_format_version: u32,
    pub address: String,
    pub name: String,
    pub friends: Vec<SuiMoveModuleId>,
    pub structs: BTreeMap<String, SuiMoveNormalizedStruct>,
    pub exposed_functions: BTreeMap<String, SuiMoveNormalizedFunction>,
}

impl From<NormalizedModule> for SuiMoveNormalizedModule {
    fn from(module: NormalizedModule) -> Self {
        Self {
            file_format_version: module.file_format_version,
            address: module.address.to_hex_literal(),
            name: module.name.to_string(),
            friends: module
                .friends
                .into_iter()
                .map(|module_id| SuiMoveModuleId {
                    address: module_id.address().to_hex_literal(),
                    name: module_id.name().to_string(),
                })
                .collect::<Vec<SuiMoveModuleId>>(),
            structs: module
                .structs
                .into_iter()
                .map(|(name, struct_)| (name.to_string(), SuiMoveNormalizedStruct::from(struct_)))
                .collect::<BTreeMap<String, SuiMoveNormalizedStruct>>(),
            exposed_functions: module
                .exposed_functions
                .into_iter()
                .map(|(name, function)| {
                    (name.to_string(), SuiMoveNormalizedFunction::from(function))
                })
                .collect::<BTreeMap<String, SuiMoveNormalizedFunction>>(),
        }
    }
}

impl From<SuiNormalizedFunction> for SuiMoveNormalizedFunction {
    fn from(function: SuiNormalizedFunction) -> Self {
        Self {
            visibility: match function.visibility {
                Visibility::Private => SuiMoveVisibility::Private,
                Visibility::Public => SuiMoveVisibility::Public,
                Visibility::Friend => SuiMoveVisibility::Friend,
            },
            is_entry: function.is_entry,
            type_parameters: function
                .type_parameters
                .into_iter()
                .map(|a| a.into())
                .collect::<Vec<SuiMoveAbilitySet>>(),
            parameters: function
                .parameters
                .into_iter()
                .map(SuiMoveNormalizedType::from)
                .collect::<Vec<SuiMoveNormalizedType>>(),
            return_: function
                .return_
                .into_iter()
                .map(SuiMoveNormalizedType::from)
                .collect::<Vec<SuiMoveNormalizedType>>(),
        }
    }
}

impl From<NormalizedStruct> for SuiMoveNormalizedStruct {
    fn from(struct_: NormalizedStruct) -> Self {
        Self {
            abilities: struct_.abilities.into(),
            type_parameters: struct_
                .type_parameters
                .into_iter()
                .map(SuiMoveStructTypeParameter::from)
                .collect::<Vec<SuiMoveStructTypeParameter>>(),
            fields: struct_
                .fields
                .into_iter()
                .map(SuiMoveNormalizedField::from)
                .collect::<Vec<SuiMoveNormalizedField>>(),
        }
    }
}

impl From<StructTypeParameter> for SuiMoveStructTypeParameter {
    fn from(type_parameter: StructTypeParameter) -> Self {
        Self {
            constraints: type_parameter.constraints.into(),
            is_phantom: type_parameter.is_phantom,
        }
    }
}

impl From<NormalizedField> for SuiMoveNormalizedField {
    fn from(normalized_field: NormalizedField) -> Self {
        Self {
            name: normalized_field.name.to_string(),
            type_: SuiMoveNormalizedType::from(normalized_field.type_),
        }
    }
}

impl From<NormalizedType> for SuiMoveNormalizedType {
    fn from(type_: NormalizedType) -> Self {
        match type_ {
            NormalizedType::Bool => SuiMoveNormalizedType::Bool,
            NormalizedType::U8 => SuiMoveNormalizedType::U8,
            NormalizedType::U64 => SuiMoveNormalizedType::U64,
            NormalizedType::U128 => SuiMoveNormalizedType::U128,
            NormalizedType::Address => SuiMoveNormalizedType::Address,
            NormalizedType::Signer => SuiMoveNormalizedType::Signer,
            NormalizedType::Struct {
                address,
                module,
                name,
                type_arguments,
            } => SuiMoveNormalizedType::Struct {
                address: address.to_hex_literal(),
                module: module.to_string(),
                name: name.to_string(),
                type_arguments: type_arguments
                    .into_iter()
                    .map(SuiMoveNormalizedType::from)
                    .collect::<Vec<SuiMoveNormalizedType>>(),
            },
            NormalizedType::Vector(v) => {
                SuiMoveNormalizedType::Vector(Box::new(SuiMoveNormalizedType::from(*v)))
            }
            NormalizedType::TypeParameter(t) => SuiMoveNormalizedType::TypeParameter(t),
            NormalizedType::Reference(r) => {
                SuiMoveNormalizedType::Reference(Box::new(SuiMoveNormalizedType::from(*r)))
            }
            NormalizedType::MutableReference(mr) => {
                SuiMoveNormalizedType::MutableReference(Box::new(SuiMoveNormalizedType::from(*mr)))
            }
        }
    }
}

impl From<AbilitySet> for SuiMoveAbilitySet {
    fn from(set: AbilitySet) -> SuiMoveAbilitySet {
        Self {
            abilities: set
                .into_iter()
                .map(|a| match a {
                    Ability::Copy => SuiMoveAbility::Copy,
                    Ability::Drop => SuiMoveAbility::Drop,
                    Ability::Key => SuiMoveAbility::Key,
                    Ability::Store => SuiMoveAbility::Store,
                })
                .collect::<Vec<SuiMoveAbility>>(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum ObjectValueKind {
    ByImmutableReference,
    ByMutableReference,
    ByValue,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum MoveFunctionArgType {
    Pure,
    Object(ObjectValueKind),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiTransactionResponse {
    pub certificate: SuiCertifiedTransaction,
    pub effects: SuiTransactionEffects,
    pub timestamp_ms: Option<u64>,
    pub parsed_data: Option<SuiParsedTransactionResponse>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum SuiParsedTransactionResponse {
    Publish(SuiParsedPublishResponse),
    MergeCoin(SuiParsedMergeCoinResponse),
    SplitCoin(SuiParsedSplitCoinResponse),
}

impl SuiParsedTransactionResponse {
    pub fn to_publish_response(self) -> Result<SuiParsedPublishResponse, SuiError> {
        match self {
            SuiParsedTransactionResponse::Publish(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_merge_coin_response(self) -> Result<SuiParsedMergeCoinResponse, SuiError> {
        match self {
            SuiParsedTransactionResponse::MergeCoin(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_split_coin_response(self) -> Result<SuiParsedSplitCoinResponse, SuiError> {
        match self {
            SuiParsedTransactionResponse::SplitCoin(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }
}

impl Display for SuiParsedTransactionResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SuiParsedTransactionResponse::Publish(r) => r.fmt(f),
            SuiParsedTransactionResponse::MergeCoin(r) => r.fmt(f),
            SuiParsedTransactionResponse::SplitCoin(r) => r.fmt(f),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum SuiExecuteTransactionResponse {
    ImmediateReturn {
        tx_digest: TransactionDigest,
    },
    TxCert {
        certificate: SuiCertifiedTransaction,
    },
    // TODO: Change to CertifiedTransactionEffects eventually.
    EffectsCert {
        certificate: SuiCertifiedTransaction,
        effects: SuiCertifiedTransactionEffects,
    },
}

impl SuiExecuteTransactionResponse {
    pub fn from_execute_transaction_response(
        resp: ExecuteTransactionResponse,
        tx_digest: TransactionDigest,
        resolver: &impl GetModule,
    ) -> Result<Self, anyhow::Error> {
        Ok(match resp {
            ExecuteTransactionResponse::ImmediateReturn => {
                SuiExecuteTransactionResponse::ImmediateReturn { tx_digest }
            }
            ExecuteTransactionResponse::TxCert(certificate) => {
                SuiExecuteTransactionResponse::TxCert {
                    certificate: (*certificate).try_into()?,
                }
            }
            ExecuteTransactionResponse::EffectsCert(cert) => {
                let (certificate, effects) = *cert;
                let certificate: SuiCertifiedTransaction = certificate.try_into()?;
                let effects: SuiCertifiedTransactionEffects =
                    SuiCertifiedTransactionEffects::try_from(effects, resolver)?;
                SuiExecuteTransactionResponse::EffectsCert {
                    certificate,
                    effects,
                }
            }
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiParsedSplitCoinResponse {
    /// The updated original coin object after split
    pub updated_coin: SuiParsedObject,
    /// All the newly created coin objects generated from the split
    pub new_coins: Vec<SuiParsedObject>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiParsedObject,
}

impl Display for SuiParsedSplitCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Split Coin Results ----".bold())?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let mut new_coin_text = Vec::new();
        for coin in &self.new_coins {
            let coin = GasCoin::try_from(coin).map_err(fmt::Error::custom)?;
            new_coin_text.push(format!("{coin}"))
        }
        writeln!(
            writer,
            "New Coins : {}",
            new_coin_text.join(",\n            ")
        )?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiParsedMergeCoinResponse {
    /// The updated original coin object after merge
    pub updated_coin: SuiParsedObject,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiParsedObject,
}

impl Display for SuiParsedMergeCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Merge Coin Results ----".bold())?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

pub type SuiRawObject = SuiObject<SuiRawMoveObject>;
pub type SuiParsedObject = SuiObject<SuiParsedMoveObject>;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "Object")]
pub struct SuiObject<T: SuiMoveObject> {
    /// The meat of the object
    pub data: SuiData<T>,
    /// The owner that unlocks this object
    pub owner: Owner,
    /// The digest of the transaction that created or last mutated this object
    pub previous_transaction: TransactionDigest,
    /// The amount of SUI we would rebate if this object gets deleted.
    /// This number is re-calculated each time the object is mutated based on
    /// the present storage gas price.
    pub storage_rebate: u64,
    pub reference: SuiObjectRef,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", rename = "ObjectRef")]
pub struct SuiObjectRef {
    /// Hex code as string representing the object id
    pub object_id: ObjectID,
    /// Object version.
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: ObjectDigest,
}

impl SuiObjectRef {
    pub fn to_object_ref(&self) -> ObjectRef {
        (self.object_id, self.version, self.digest)
    }
}

impl From<ObjectRef> for SuiObjectRef {
    fn from(oref: ObjectRef) -> Self {
        Self {
            object_id: oref.0,
            version: oref.1,
            digest: oref.2,
        }
    }
}

impl Display for SuiParsedObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let type_ = if self.data.type_().is_some() {
            "Move Object"
        } else {
            "Move Package"
        };
        let mut writer = String::new();
        writeln!(
            writer,
            "{}",
            format!(
                "----- {type_} ({}[{}]) -----",
                self.id(),
                self.version().value()
            )
            .bold()
        )?;
        writeln!(writer, "{}: {}", "Owner".bold().bright_black(), self.owner)?;
        writeln!(
            writer,
            "{}: {}",
            "Version".bold().bright_black(),
            self.version().value()
        )?;
        writeln!(
            writer,
            "{}: {}",
            "Storage Rebate".bold().bright_black(),
            self.storage_rebate
        )?;
        writeln!(
            writer,
            "{}: {:?}",
            "Previous Transaction".bold().bright_black(),
            self.previous_transaction
        )?;
        writeln!(writer, "{}", "----- Data -----".bold())?;
        write!(writer, "{}", &self.data)?;
        write!(f, "{}", writer)
    }
}

impl<T: SuiMoveObject> SuiObject<T> {
    pub fn id(&self) -> ObjectID {
        self.reference.object_id
    }
    pub fn version(&self) -> SequenceNumber {
        self.reference.version
    }

    pub fn try_from(o: Object, layout: Option<MoveStructLayout>) -> Result<Self, anyhow::Error> {
        let oref = o.compute_object_reference();
        let data = match o.data {
            Data::Move(m) => {
                let layout = layout.ok_or(SuiError::ObjectSerializationError {
                    error: "Layout is required to convert Move object to json".to_owned(),
                })?;
                SuiData::MoveObject(T::try_from_layout(m, layout)?)
            }
            Data::Package(p) => SuiData::Package(SuiMovePackage {
                disassembled: p.disassemble()?,
            }),
        };
        Ok(Self {
            data,
            owner: o.owner,
            previous_transaction: o.previous_transaction,
            storage_rebate: o.storage_rebate,
            reference: oref.into(),
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(tag = "dataType", rename_all = "camelCase", rename = "Data")]
pub enum SuiData<T: SuiMoveObject> {
    // Manually handle generic schema generation
    MoveObject(#[schemars(with = "Either<SuiParsedMoveObject,SuiRawMoveObject>")] T),
    Package(SuiMovePackage),
}

impl Display for SuiData<SuiParsedMoveObject> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match self {
            SuiData::MoveObject(o) => {
                writeln!(writer, "{}: {}", "type".bold().bright_black(), o.type_)?;
                write!(writer, "{}", &o.fields)?;
            }
            SuiData::Package(p) => {
                write!(
                    writer,
                    "{}: {:?}",
                    "Modules".bold().bright_black(),
                    p.disassembled.keys()
                )?;
            }
        }
        write!(f, "{}", writer)
    }
}

fn indent<T: Display>(d: &T, indent: usize) -> String {
    d.to_string()
        .lines()
        .map(|line| format!("{:indent$}{}", "", line))
        .join("\n")
}

pub trait SuiMoveObject: Sized {
    fn try_from_layout(object: MoveObject, layout: MoveStructLayout)
        -> Result<Self, anyhow::Error>;

    fn try_from(o: MoveObject, resolver: &impl GetModule) -> Result<Self, anyhow::Error> {
        let layout = o.get_layout(ObjectFormatOptions::default(), resolver)?;
        Self::try_from_layout(o, layout)
    }

    fn type_(&self) -> &str;
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MoveObject")]
pub struct SuiParsedMoveObject {
    #[serde(rename = "type")]
    pub type_: String,
    pub has_public_transfer: bool,
    pub fields: SuiMoveStruct,
}

impl SuiMoveObject for SuiParsedMoveObject {
    fn try_from_layout(
        object: MoveObject,
        layout: MoveStructLayout,
    ) -> Result<Self, anyhow::Error> {
        let move_struct = object.to_move_struct(&layout)?.into();

        Ok(
            if let SuiMoveStruct::WithTypes { type_, fields } = move_struct {
                SuiParsedMoveObject {
                    type_,
                    has_public_transfer: object.has_public_transfer(),
                    fields: SuiMoveStruct::WithFields(fields),
                }
            } else {
                SuiParsedMoveObject {
                    type_: object.type_.to_string(),
                    has_public_transfer: object.has_public_transfer(),
                    fields: move_struct,
                }
            },
        )
    }

    fn type_(&self) -> &str {
        &self.type_
    }
}

impl SuiParsedMoveObject {
    fn try_type_and_fields_from_move_struct(
        type_: &StructTag,
        move_struct: MoveStruct,
    ) -> Result<(String, SuiMoveStruct), anyhow::Error> {
        Ok(match move_struct.into() {
            SuiMoveStruct::WithTypes { type_, fields } => {
                (type_, SuiMoveStruct::WithFields(fields))
            }
            fields => (type_.to_string(), fields),
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "RawMoveObject")]
pub struct SuiRawMoveObject {
    #[serde(rename = "type")]
    pub type_: String,
    pub has_public_transfer: bool,
    #[serde_as(as = "Base64")]
    #[schemars(with = "Base64")]
    pub bcs_bytes: Vec<u8>,
}

impl SuiMoveObject for SuiRawMoveObject {
    fn try_from_layout(
        object: MoveObject,
        _layout: MoveStructLayout,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            type_: object.type_.to_string(),
            has_public_transfer: object.has_public_transfer(),
            bcs_bytes: object.into_contents(),
        })
    }

    fn type_(&self) -> &str {
        &self.type_
    }
}

impl TryFrom<&SuiParsedObject> for GasCoin {
    type Error = SuiError;
    fn try_from(object: &SuiParsedObject) -> Result<Self, Self::Error> {
        match &object.data {
            SuiData::MoveObject(o) => {
                if GasCoin::type_().to_string() == o.type_ {
                    return GasCoin::try_from(&o.fields);
                }
            }
            SuiData::Package(_) => {}
        }

        Err(SuiError::TypeError {
            error: format!(
                "Gas object type is not a gas coin: {:?}",
                object.data.type_()
            ),
        })
    }
}

impl TryFrom<&SuiMoveStruct> for GasCoin {
    type Error = SuiError;
    fn try_from(move_struct: &SuiMoveStruct) -> Result<Self, Self::Error> {
        match move_struct {
            SuiMoveStruct::WithFields(fields) | SuiMoveStruct::WithTypes { type_: _, fields } => {
                if let Some(SuiMoveValue::Number(balance)) = fields.get("balance") {
                    if let Some(SuiMoveValue::UID { id }) = fields.get("id") {
                        return Ok(GasCoin::new(*id, *balance));
                    }
                }
            }
            _ => {}
        }
        Err(SuiError::TypeError {
            error: format!("Struct is not a gas coin: {move_struct:?}"),
        })
    }
}

impl<T: SuiMoveObject> SuiData<T> {
    pub fn try_as_move(&self) -> Option<&T> {
        match self {
            SuiData::MoveObject(o) => Some(o),
            SuiData::Package(_) => None,
        }
    }
    pub fn try_as_package(&self) -> Option<&SuiMovePackage> {
        match self {
            SuiData::MoveObject(_) => None,
            SuiData::Package(p) => Some(p),
        }
    }
    pub fn type_(&self) -> Option<&str> {
        match self {
            SuiData::MoveObject(m) => Some(m.type_()),
            SuiData::Package(_) => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiParsedPublishResponse {
    /// The newly published package object reference.
    pub package: SuiObjectRef,
    /// List of Move objects created as part of running the module initializers in the package
    pub created_objects: Vec<SuiParsedObject>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiParsedObject,
}

impl Display for SuiParsedPublishResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Publish Results ----".bold())?;
        writeln!(
            writer,
            "{}",
            format!(
                "The newly published package object ID: {:?}\n",
                self.package.object_id
            )
            .bold()
        )?;
        if !self.created_objects.is_empty() {
            writeln!(
                writer,
                "List of objects created by running module initializers:"
            )?;
            for obj in &self.created_objects {
                writeln!(writer, "{}\n", obj)?;
            }
        }
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

pub type GetObjectDataResponse = SuiObjectRead<SuiParsedMoveObject>;
pub type GetRawObjectDataResponse = SuiObjectRead<SuiRawMoveObject>;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(tag = "status", content = "details", rename = "ObjectRead")]
pub enum SuiObjectRead<T: SuiMoveObject> {
    Exists(SuiObject<T>),
    NotExists(ObjectID),
    Deleted(SuiObjectRef),
}

impl<T: SuiMoveObject> SuiObjectRead<T> {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> Result<&SuiObject<T>, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(o) => Ok(o),
        }
    }

    /// Returns the object value if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn into_object(self) -> Result<SuiObject<T>, SuiError> {
        match self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: id }),
            Self::Exists(o) => Ok(o),
        }
    }
}

impl<T: SuiMoveObject> TryFrom<ObjectRead> for SuiObjectRead<T> {
    type Error = anyhow::Error;

    fn try_from(value: ObjectRead) -> Result<Self, Self::Error> {
        match value {
            ObjectRead::NotExists(id) => Ok(SuiObjectRead::NotExists(id)),
            ObjectRead::Exists(_, o, layout) => {
                Ok(SuiObjectRead::Exists(SuiObject::try_from(o, layout)?))
            }
            ObjectRead::Deleted(oref) => Ok(SuiObjectRead::Deleted(oref.into())),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(untagged, rename = "MoveValue")]
pub enum SuiMoveValue {
    Number(u64),
    Bool(bool),
    Address(SuiAddress),
    Vector(Vec<SuiMoveValue>),
    Bytearray(Base64),
    String(String),
    UID { id: ObjectID },
    Struct(SuiMoveStruct),
    Option(Box<Option<SuiMoveValue>>),
}

impl Display for SuiMoveValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match self {
            SuiMoveValue::Number(value) => {
                write!(writer, "{}", value)?;
            }
            SuiMoveValue::Bool(value) => {
                write!(writer, "{}", value)?;
            }
            SuiMoveValue::Address(value) => {
                write!(writer, "{}", value)?;
            }
            SuiMoveValue::Vector(vec) => {
                write!(
                    writer,
                    "{}",
                    vec.iter().map(|value| format!("{value}")).join(",\n")
                )?;
            }
            SuiMoveValue::String(value) => {
                write!(writer, "{}", value)?;
            }
            SuiMoveValue::UID { id } => {
                write!(writer, "{id}")?;
            }
            SuiMoveValue::Struct(value) => {
                write!(writer, "{}", value)?;
            }
            SuiMoveValue::Option(value) => {
                write!(writer, "{:?}", value)?;
            }
            SuiMoveValue::Bytearray(value) => {
                write!(
                    writer,
                    "{:?}",
                    value.clone().to_vec().map_err(fmt::Error::custom)?
                )?;
            }
        }

        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

impl From<MoveValue> for SuiMoveValue {
    fn from(value: MoveValue) -> Self {
        match value {
            MoveValue::U8(value) => SuiMoveValue::Number(value.into()),
            MoveValue::U64(value) => SuiMoveValue::Number(value),
            MoveValue::U128(value) => SuiMoveValue::String(format!("{value}")),
            MoveValue::Bool(value) => SuiMoveValue::Bool(value),
            MoveValue::Vector(value) => {
                // Try convert bytearray
                if value.iter().all(|value| matches!(value, MoveValue::U8(_))) {
                    let bytearray = value
                        .iter()
                        .flat_map(|value| {
                            if let MoveValue::U8(u8) = value {
                                Some(*u8)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    return SuiMoveValue::Bytearray(Base64::from_bytes(&bytearray));
                }
                SuiMoveValue::Vector(value.into_iter().map(|value| value.into()).collect())
            }
            MoveValue::Struct(value) => {
                // Best effort Sui core type conversion
                if let MoveStruct::WithTypes { type_, fields } = &value {
                    if let Some(value) = try_convert_type(type_, fields) {
                        return value;
                    }
                };
                SuiMoveValue::Struct(value.into())
            }
            MoveValue::Signer(value) | MoveValue::Address(value) => {
                SuiMoveValue::Address(SuiAddress::from(ObjectID::from(value)))
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(untagged, rename = "MoveStruct")]
pub enum SuiMoveStruct {
    Runtime(Vec<SuiMoveValue>),
    WithTypes {
        #[serde(rename = "type")]
        type_: String,
        fields: BTreeMap<String, SuiMoveValue>,
    },
    WithFields(BTreeMap<String, SuiMoveValue>),
}

impl SuiMoveStruct {
    pub fn to_json_value(self) -> Result<Value, serde_json::Error> {
        // Unwrap MoveStructs
        let unwrapped = match self {
            SuiMoveStruct::Runtime(values) => {
                let values = values
                    .into_iter()
                    .map(|value| match value {
                        SuiMoveValue::Struct(move_struct) => move_struct.to_json_value(),
                        SuiMoveValue::Vector(values) => {
                            SuiMoveStruct::Runtime(values).to_json_value()
                        }
                        _ => serde_json::to_value(&value),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_value(&values)
            }
            // We only care about values here, assuming struct type information is known at the client side.
            SuiMoveStruct::WithTypes { type_: _, fields } | SuiMoveStruct::WithFields(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|(key, value)| {
                        let value = match value {
                            SuiMoveValue::Struct(move_struct) => move_struct.to_json_value(),
                            SuiMoveValue::Vector(values) => {
                                SuiMoveStruct::Runtime(values).to_json_value()
                            }
                            _ => serde_json::to_value(&value),
                        };
                        value.map(|value| (key, value))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;
                serde_json::to_value(&fields)
            }
        }?;
        serde_json::to_value(&unwrapped)
    }
}

impl Display for SuiMoveStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match self {
            SuiMoveStruct::Runtime(_) => {}
            SuiMoveStruct::WithFields(fields) => {
                for (name, value) in fields {
                    writeln!(writer, "{}: {value}", name.bold().bright_black())?;
                }
            }
            SuiMoveStruct::WithTypes { type_, fields } => {
                writeln!(writer)?;
                writeln!(writer, "  {}: {type_}", "type".bold().bright_black())?;
                for (name, value) in fields {
                    let value = format!("{}", value);
                    let value = if value.starts_with('\n') {
                        indent(&value, 2)
                    } else {
                        value
                    };
                    writeln!(writer, "  {}: {value}", name.bold().bright_black())?;
                }
            }
        }
        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

fn try_convert_type(type_: &StructTag, fields: &[(Identifier, MoveValue)]) -> Option<SuiMoveValue> {
    let struct_name = format!(
        "0x{}::{}::{}",
        type_.address.short_str_lossless(),
        type_.module,
        type_.name
    );
    let fields = fields
        .iter()
        .map(|(id, value)| (id.to_string(), value.clone().into()))
        .collect::<BTreeMap<_, SuiMoveValue>>();
    match struct_name.as_str() {
        "0x2::utf8::String" | "0x1::ascii::String" => {
            if let Some(SuiMoveValue::Bytearray(bytes)) = fields.get("bytes") {
                if let Ok(bytes) = bytes.to_vec() {
                    if let Ok(s) = String::from_utf8(bytes) {
                        return Some(SuiMoveValue::String(s));
                    }
                }
            }
        }
        "0x2::url::Url" => {
            if let Some(url) = fields.get("url") {
                return Some(url.clone());
            }
        }
        "0x2::object::ID" => {
            if let Some(SuiMoveValue::Address(id)) = fields.get("bytes") {
                return Some(SuiMoveValue::Address(*id));
            }
        }
        "0x2::object::UID" => {
            if let Some(SuiMoveValue::Address(address)) = fields.get("id") {
                return Some(SuiMoveValue::UID {
                    id: ObjectID::from(*address),
                });
            }
        }
        "0x2::balance::Balance" => {
            if let Some(SuiMoveValue::Number(value)) = fields.get("value") {
                return Some(SuiMoveValue::Number(*value));
            }
        }
        "0x1::option::Option" => {
            if let Some(SuiMoveValue::Vector(values)) = fields.get("vec") {
                return Some(SuiMoveValue::Option(Box::new(values.first().cloned())));
            }
        }
        _ => return None,
    }
    warn!(
        fields =? fields,
        "Failed to convert {struct_name} to SuiMoveValue"
    );
    None
}

impl From<MoveStruct> for SuiMoveStruct {
    fn from(move_struct: MoveStruct) -> Self {
        match move_struct {
            MoveStruct::Runtime(value) => {
                SuiMoveStruct::Runtime(value.into_iter().map(|value| value.into()).collect())
            }
            MoveStruct::WithFields(value) => SuiMoveStruct::WithFields(
                value
                    .into_iter()
                    .map(|(id, value)| (id.into_string(), value.into()))
                    .collect(),
            ),
            MoveStruct::WithTypes { type_, fields } => SuiMoveStruct::WithTypes {
                type_: type_.to_string(),
                fields: fields
                    .into_iter()
                    .map(|(id, value)| (id.into_string(), value.into()))
                    .collect(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MovePackage")]
pub struct SuiMovePackage {
    disassembled: BTreeMap<String, Value>,
}

impl TryFrom<MoveModulePublish> for SuiMovePackage {
    type Error = anyhow::Error;

    fn try_from(m: MoveModulePublish) -> Result<Self, Self::Error> {
        Ok(Self {
            disassembled: disassemble_modules(m.modules.iter())?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone)]
#[serde(rename = "TransactionData", rename_all = "camelCase")]
pub struct SuiTransactionData {
    pub transactions: Vec<SuiTransactionKind>,
    pub sender: SuiAddress,
    pub gas_payment: SuiObjectRef,
    pub gas_budget: u64,
}

impl Display for SuiTransactionData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        if self.transactions.len() == 1 {
            writeln!(writer, "{}", self.transactions.first().unwrap())?;
        } else {
            writeln!(writer, "Transaction Kind : Batch")?;
            writeln!(writer, "List of transactions in the batch:")?;
            for kind in &self.transactions {
                writeln!(writer, "{}", kind)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl TryFrom<TransactionData> for SuiTransactionData {
    type Error = anyhow::Error;

    fn try_from(data: TransactionData) -> Result<Self, Self::Error> {
        let transactions = match data.kind.clone() {
            TransactionKind::Single(tx) => {
                vec![tx.try_into()?]
            }
            TransactionKind::Batch(txs) => txs
                .into_iter()
                .map(SuiTransactionKind::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(Self {
            transactions,
            sender: data.signer(),
            gas_payment: data.gas().into(),
            gas_budget: data.gas_budget,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransactionKind")]
pub enum SuiTransactionKind {
    /// Initiate an object transfer between addresses
    TransferObject(SuiTransferObject),
    /// Publish a new Move module
    Publish(SuiMovePackage),
    /// Call a function in a published Move module
    Call(SuiMoveCall),
    /// Initiate a SUI coin transfer between addresses
    TransferSui(SuiTransferSui),
    /// A system transaction that will update epoch information on-chain.
    ChangeEpoch(SuiChangeEpoch),
    // .. more transaction types go here
}

impl Display for SuiTransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match &self {
            Self::TransferObject(t) => {
                writeln!(writer, "Transaction Kind : Transfer Object")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                writeln!(writer, "Object ID : {}", t.object_ref.object_id)?;
                writeln!(writer, "Version : {:?}", t.object_ref.version)?;
                write!(
                    writer,
                    "Object Digest : {}",
                    Base64::encode(t.object_ref.digest)
                )?;
            }
            Self::TransferSui(t) => {
                writeln!(writer, "Transaction Kind : Transfer SUI")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                if let Some(amount) = t.amount {
                    writeln!(writer, "Amount: {}", amount)?;
                } else {
                    writeln!(writer, "Amount: Full Balance")?;
                }
            }
            Self::Publish(_p) => {
                write!(writer, "Transaction Kind : Publish")?;
            }
            Self::Call(c) => {
                writeln!(writer, "Transaction Kind : Call")?;
                writeln!(
                    writer,
                    "Package ID : {}",
                    c.package.object_id.to_hex_literal()
                )?;
                writeln!(writer, "Module : {}", c.module)?;
                writeln!(writer, "Function : {}", c.function)?;
                writeln!(writer, "Arguments : {:?}", c.arguments)?;
                write!(writer, "Type Arguments : {:?}", c.type_arguments)?;
            }
            Self::ChangeEpoch(e) => {
                writeln!(writer, "Transaction Kind: Epoch Change")?;
                writeln!(writer, "New epoch ID: {}", e.epoch)?;
                writeln!(writer, "Storage gas reward: {}", e.storage_charge)?;
                writeln!(writer, "Computation gas reward: {}", e.computation_charge)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl TryFrom<SingleTransactionKind> for SuiTransactionKind {
    type Error = anyhow::Error;

    fn try_from(tx: SingleTransactionKind) -> Result<Self, Self::Error> {
        Ok(match tx {
            SingleTransactionKind::TransferObject(t) => Self::TransferObject(SuiTransferObject {
                recipient: t.recipient,
                object_ref: t.object_ref.into(),
            }),
            SingleTransactionKind::TransferSui(t) => Self::TransferSui(SuiTransferSui {
                recipient: t.recipient,
                amount: t.amount,
            }),
            SingleTransactionKind::Publish(p) => Self::Publish(p.try_into()?),
            SingleTransactionKind::Call(c) => Self::Call(SuiMoveCall {
                package: c.package.into(),
                module: c.module.to_string(),
                function: c.function.to_string(),
                type_arguments: c.type_arguments.iter().map(|ty| ty.to_string()).collect(),
                arguments: c
                    .arguments
                    .into_iter()
                    .map(|arg| match arg {
                        CallArg::Pure(p) => SuiJsonValue::from_bcs_bytes(&p),
                        CallArg::Object(ObjectArg::ImmOrOwnedObject((id, _, _))) => {
                            SuiJsonValue::new(Value::String(id.to_hex_literal()))
                        }
                        CallArg::Object(ObjectArg::SharedObject(id)) => {
                            SuiJsonValue::new(Value::String(id.to_hex_literal()))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            SingleTransactionKind::ChangeEpoch(e) => Self::ChangeEpoch(SuiChangeEpoch {
                epoch: e.epoch,
                storage_charge: e.storage_charge,
                computation_charge: e.computation_charge,
            }),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "MoveCall", rename_all = "camelCase")]
pub struct SuiMoveCall {
    pub package: SuiObjectRef,
    pub module: String,
    pub function: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<SuiJsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SuiChangeEpoch {
    pub epoch: EpochId,
    pub storage_charge: u64,
    pub computation_charge: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "CertifiedTransaction", rename_all = "camelCase")]
pub struct SuiCertifiedTransaction {
    pub transaction_digest: TransactionDigest,
    pub data: SuiTransactionData,
    /// tx_signature is signed by the transaction sender, applied on `data`.
    pub tx_signature: Signature,
    /// authority signature information, if available, is signed by an authority, applied on `data`.
    pub auth_sign_info: AuthorityStrongQuorumSignInfo,
}

impl Display for SuiCertifiedTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Transaction Hash: {:?}", self.transaction_digest)?;
        writeln!(writer, "Transaction Signature: {:?}", self.tx_signature)?;
        writeln!(
            writer,
            "Signed Authorities Bitmap: {:?}",
            self.auth_sign_info.signers_map
        )?;
        write!(writer, "{}", &self.data)?;
        write!(f, "{}", writer)
    }
}

impl TryFrom<CertifiedTransaction> for SuiCertifiedTransaction {
    type Error = anyhow::Error;

    fn try_from(cert: CertifiedTransaction) -> Result<Self, Self::Error> {
        let digest = *cert.digest();
        let auth_sign_info = cert.auth_sig().clone();
        let SenderSignedData { data, tx_signature } = cert.into_data();
        Ok(Self {
            transaction_digest: digest,
            tx_signature,
            auth_sign_info,
            data: data.try_into()?,
        })
    }
}

/// The certified Transaction Effects which has signatures from >= 2/3 of validators
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "CertifiedTransactionEffects", rename_all = "camelCase")]
pub struct SuiCertifiedTransactionEffects {
    pub transaction_effects_digest: TransactionEffectsDigest,
    pub effects: SuiTransactionEffects,
    /// authority signature information signed by the quorum of the validators.
    pub auth_sign_info: AuthorityStrongQuorumSignInfo,
}

impl Display for SuiCertifiedTransactionEffects {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(
            writer,
            "Transaction Effects Digest: {:?}",
            self.transaction_effects_digest
        )?;
        writeln!(writer, "Transaction Effects: {:?}", self.effects)?;
        writeln!(
            writer,
            "Signed Authorities Bitmap: {:?}",
            self.auth_sign_info.signers_map
        )?;
        write!(f, "{}", writer)
    }
}

impl SuiCertifiedTransactionEffects {
    fn try_from(
        cert: CertifiedTransactionEffects,
        resolver: &impl GetModule,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            transaction_effects_digest: *cert.digest(),
            effects: SuiTransactionEffects::try_from(cert.data().clone(), resolver)?,
            auth_sign_info: cert.auth_signature,
        })
    }
}

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransactionEffects", rename_all = "camelCase")]
pub struct SuiTransactionEffects {
    // The status of the execution
    pub status: SuiExecutionStatus,
    pub gas_used: SuiGasCostSummary,
    // The object references of the shared objects used in this transaction. Empty if no shared objects were used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_objects: Vec<SuiObjectRef>,
    // The transaction digest
    pub transaction_digest: TransactionDigest,
    // ObjectRef and owner of new objects created.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created: Vec<OwnedObjectRef>,
    // ObjectRef and owner of mutated objects, including gas object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutated: Vec<OwnedObjectRef>,
    // ObjectRef and owner of objects that are unwrapped in this transaction.
    // Unwrapped objects are objects that were wrapped into other objects in the past,
    // and just got extracted out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unwrapped: Vec<OwnedObjectRef>,
    // Object Refs of objects now deleted (the old refs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<SuiObjectRef>,
    // Object refs of objects now wrapped in other objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrapped: Vec<SuiObjectRef>,
    // The updated gas object reference. Have a dedicated field for convenient access.
    // It's also included in mutated.
    pub gas_object: OwnedObjectRef,
    /// The events emitted during execution. Note that only successful transactions emit events
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<SuiEvent>,
    /// The set of transaction digests this transaction depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<TransactionDigest>,
}

impl SuiTransactionEffects {
    /// Return an iterator of mutated objects, but excluding the gas object.
    pub fn mutated_excluding_gas(&self) -> impl Iterator<Item = &OwnedObjectRef> {
        self.mutated.iter().filter(|o| *o != &self.gas_object)
    }

    pub fn try_from(
        effect: TransactionEffects,
        resolver: &impl GetModule,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            status: effect.status.into(),
            gas_used: effect.gas_used.into(),
            shared_objects: to_sui_object_ref(effect.shared_objects),
            transaction_digest: effect.transaction_digest,
            created: to_owned_ref(effect.created),
            mutated: to_owned_ref(effect.mutated),
            unwrapped: to_owned_ref(effect.unwrapped),
            deleted: to_sui_object_ref(effect.deleted),
            wrapped: to_sui_object_ref(effect.wrapped),
            gas_object: OwnedObjectRef {
                owner: effect.gas_object.1,
                reference: effect.gas_object.0.into(),
            },
            events: effect
                .events
                .into_iter()
                .map(|event| SuiEvent::try_from(event, resolver))
                .collect::<Result<_, _>>()?,
            dependencies: effect.dependencies,
        })
    }
}

impl Display for SuiTransactionEffects {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Status : {:?}", self.status)?;
        if !self.created.is_empty() {
            writeln!(writer, "Created Objects:")?;
            for oref in &self.created {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        if !self.mutated.is_empty() {
            writeln!(writer, "Mutated Objects:")?;
            for oref in &self.mutated {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        if !self.deleted.is_empty() {
            writeln!(writer, "Deleted Objects:")?;
            for oref in &self.deleted {
                writeln!(writer, "  - ID: {}", oref.object_id)?;
            }
        }
        if !self.wrapped.is_empty() {
            writeln!(writer, "Wrapped Objects:")?;
            for oref in &self.wrapped {
                writeln!(writer, "  - ID: {}", oref.object_id)?;
            }
        }
        if !self.unwrapped.is_empty() {
            writeln!(writer, "Unwrapped Objects:")?;
            for oref in &self.unwrapped {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        write!(f, "{}", writer)
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "ExecutionStatus", rename_all = "camelCase", tag = "status")]
pub enum SuiExecutionStatus {
    // Gas used in the success case.
    Success,
    // Gas used in the failed case, and the error.
    Failure { error: String },
}

impl SuiExecutionStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, SuiExecutionStatus::Success { .. })
    }
    pub fn is_err(&self) -> bool {
        matches!(self, SuiExecutionStatus::Failure { .. })
    }
}

impl From<ExecutionStatus> for SuiExecutionStatus {
    fn from(status: ExecutionStatus) -> Self {
        match status {
            ExecutionStatus::Success => Self::Success,
            ExecutionStatus::Failure { error } => Self::Failure {
                error: format!("{:?}", error),
            },
        }
    }
}

fn to_sui_object_ref(refs: Vec<ObjectRef>) -> Vec<SuiObjectRef> {
    refs.into_iter().map(SuiObjectRef::from).collect()
}

fn to_owned_ref(owned_refs: Vec<(ObjectRef, Owner)>) -> Vec<OwnedObjectRef> {
    owned_refs
        .into_iter()
        .map(|(oref, owner)| OwnedObjectRef {
            owner,
            reference: oref.into(),
        })
        .collect()
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "GasCostSummary", rename_all = "camelCase")]
pub struct SuiGasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
}

impl From<GasCostSummary> for SuiGasCostSummary {
    fn from(s: GasCostSummary) -> Self {
        Self {
            computation_cost: s.computation_cost,
            storage_cost: s.storage_cost,
            storage_rebate: s.storage_rebate,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "ObjectRef")]
pub struct OwnedObjectRef {
    pub owner: Owner,
    pub reference: SuiObjectRef,
}

#[serde_as]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "EventEnvelope", rename_all = "camelCase")]
pub struct SuiEventEnvelope {
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    pub timestamp: u64,
    /// Transaction digest of associated transaction, if any
    pub tx_digest: Option<TransactionDigest>,
    /// Specific event type
    pub event: SuiEvent,
}

#[serde_as]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "Event", rename_all = "camelCase")]
pub enum SuiEvent {
    /// Move-specific event
    #[serde(rename_all = "camelCase")]
    MoveEvent {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        type_: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<SuiMoveStruct>,
        #[serde_as(as = "Base64")]
        #[schemars(with = "Base64")]
        bcs: Vec<u8>,
    },
    /// Module published
    #[serde(rename_all = "camelCase")]
    Publish {
        sender: SuiAddress,
        package_id: ObjectID,
    },
    /// Transfer objects to new address / wrap in another object / coin
    #[serde(rename_all = "camelCase")]
    TransferObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        recipient: Owner,
        object_id: ObjectID,
        version: SequenceNumber,
        type_: TransferType,
    },
    /// Delete object
    #[serde(rename_all = "camelCase")]
    DeleteObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        object_id: ObjectID,
    },
    /// New object creation
    #[serde(rename_all = "camelCase")]
    NewObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        recipient: Owner,
        object_id: ObjectID,
    },
    /// Epoch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
}

impl SuiEvent {
    pub fn try_from(event: Event, resolver: &impl GetModule) -> Result<Self, anyhow::Error> {
        Ok(match event {
            Event::MoveEvent {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } => {
                let bcs = contents.to_vec();

                // Resolver is not guaranteed to have knowledge of the event struct layout in the gateway server.
                let (type_, fields) = if let Ok(move_struct) =
                    Event::move_event_to_move_struct(&type_, &contents, resolver)
                {
                    let (type_, field) = SuiParsedMoveObject::try_type_and_fields_from_move_struct(
                        &type_,
                        move_struct,
                    )?;
                    (type_, Some(field))
                } else {
                    (type_.to_string(), None)
                };

                SuiEvent::MoveEvent {
                    package_id,
                    transaction_module: transaction_module.to_string(),
                    sender,
                    type_,
                    fields,
                    bcs,
                }
            }
            Event::Publish { sender, package_id } => SuiEvent::Publish { sender, package_id },
            Event::TransferObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_id,
                version,
                type_,
            } => SuiEvent::TransferObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                recipient,
                object_id,
                version,
                type_,
            },
            Event::DeleteObject {
                package_id,
                transaction_module,
                sender,
                object_id,
            } => SuiEvent::DeleteObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                object_id,
            },
            Event::NewObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_id,
            } => SuiEvent::NewObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                recipient,
                object_id,
            },
            Event::EpochChange(id) => SuiEvent::EpochChange(id),
            Event::Checkpoint(seq) => SuiEvent::Checkpoint(seq),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransferObject", rename_all = "camelCase")]
pub struct SuiTransferObject {
    pub recipient: SuiAddress,
    pub object_ref: SuiObjectRef,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransferSui", rename_all = "camelCase")]
pub struct SuiTransferSui {
    pub recipient: SuiAddress,
    pub amount: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "InputObjectKind")]
pub enum SuiInputObjectKind {
    // A Move package, must be immutable.
    MovePackage(ObjectID),
    // A Move object, either immutable, or owned mutable.
    ImmOrOwnedMoveObject(SuiObjectRef),
    // A Move object that's shared and mutable.
    SharedMoveObject(ObjectID),
}

impl From<InputObjectKind> for SuiInputObjectKind {
    fn from(input: InputObjectKind) -> Self {
        match input {
            InputObjectKind::MovePackage(id) => Self::MovePackage(id),
            InputObjectKind::ImmOrOwnedMoveObject(oref) => Self::ImmOrOwnedMoveObject(oref.into()),
            InputObjectKind::SharedMoveObject(id) => Self::SharedMoveObject(id),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, Ord, PartialOrd, Eq, PartialEq, Debug)]
#[serde(rename = "ObjectInfo", rename_all = "camelCase")]
pub struct SuiObjectInfo {
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    #[serde(rename = "type")]
    pub type_: String,
    pub owner: Owner,
    pub previous_transaction: TransactionDigest,
}

impl SuiObjectInfo {
    pub fn to_object_ref(&self) -> ObjectRef {
        (self.object_id, self.version, self.digest)
    }
}

impl From<ObjectInfo> for SuiObjectInfo {
    fn from(info: ObjectInfo) -> Self {
        Self {
            object_id: info.object_id,
            version: info.version,
            digest: info.digest,
            type_: info.type_,
            owner: info.owner,
            previous_transaction: info.previous_transaction,
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectExistsResponse {
    object_ref: SuiObjectRef,
    owner: Owner,
    previous_transaction: TransactionDigest,
    data: SuiData<SuiParsedMoveObject>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectNotExistsResponse {
    object_id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TypeTag", rename_all = "camelCase")]
pub struct SuiTypeTag(String);

impl TryInto<TypeTag> for SuiTypeTag {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<TypeTag, Self::Error> {
        parse_type_tag(&self.0)
    }
}

impl From<TypeTag> for SuiTypeTag {
    fn from(tag: TypeTag) -> Self {
        Self(format!("{}", tag))
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RPCTransactionRequestParams {
    TransferObjectRequestParams(TransferObjectParams),
    MoveCallRequestParams(MoveCallParams),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferObjectParams {
    pub recipient: SuiAddress,
    pub object_id: ObjectID,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveCallParams {
    pub package_object_id: ObjectID,
    pub module: String,
    pub function: String,
    #[serde(default)]
    pub type_arguments: Vec<SuiTypeTag>,
    pub arguments: Vec<SuiJsonValue>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename = "EventFilter")]
pub enum SuiEventFilter {
    Package(ObjectID),
    Module(String),
    /// Move StructTag string value of the event type e.g. `0x2::devnet_nft::MintNFTEvent`
    MoveEventType(String),
    MoveEventField {
        path: String,
        value: Value,
    },
    SenderAddress(SuiAddress),
    EventType(EventType),
    ObjectId(ObjectID),
    All(Vec<SuiEventFilter>),
    Any(Vec<SuiEventFilter>),
    And(Box<SuiEventFilter>, Box<SuiEventFilter>),
    Or(Box<SuiEventFilter>, Box<SuiEventFilter>),
}

impl TryInto<EventFilter> for SuiEventFilter {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<EventFilter, anyhow::Error> {
        use SuiEventFilter::*;
        Ok(match self {
            Package(id) => EventFilter::Package(id),
            Module(module) => EventFilter::Module(Identifier::new(module)?),
            MoveEventType(event_type) => {
                // parse_struct_tag converts StructTag string e.g. `0x2::devnet_nft::MintNFTEvent` to StructTag object
                EventFilter::MoveEventType(parse_struct_tag(&event_type)?)
            }
            MoveEventField { path, value } => EventFilter::MoveEventField { path, value },
            SenderAddress(address) => EventFilter::SenderAddress(address),
            ObjectId(id) => EventFilter::ObjectId(id),
            All(filters) => EventFilter::MatchAll(
                filters
                    .into_iter()
                    .map(SuiEventFilter::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            Any(filters) => EventFilter::MatchAny(
                filters
                    .into_iter()
                    .map(SuiEventFilter::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            And(filter_a, filter_b) => All(vec![*filter_a, *filter_b]).try_into()?,
            Or(filter_a, filter_b) => Any(vec![*filter_a, *filter_b]).try_into()?,
            EventType(type_) => EventFilter::EventType(type_),
        })
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionBytes {
    /// transaction data bytes, as base-64 encoded string
    pub tx_bytes: Base64,
    /// the gas object to be used
    pub gas: SuiObjectRef,
    /// objects to be used in this transaction
    pub input_objects: Vec<SuiInputObjectKind>,
}

impl TransactionBytes {
    pub fn from_data(data: TransactionData) -> Result<Self, anyhow::Error> {
        Ok(Self {
            tx_bytes: Base64::from_bytes(&data.to_bytes()),
            gas: data.gas().into(),
            input_objects: data
                .input_objects()?
                .into_iter()
                .map(SuiInputObjectKind::from)
                .collect(),
        })
    }

    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        TransactionData::from_signable_bytes(&self.tx_bytes.to_vec()?)
    }
}
