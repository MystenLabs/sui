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
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use schemars::JsonSchema;
use serde::ser::Error;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::crypto::{AuthorityQuorumSignInfo, Signature};
use sui_types::error::SuiError;
use sui_types::event::Event;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, CertifiedTransaction, ExecutionStatus, InputObjectKind, MoveModulePublish,
    SingleTransactionKind, TransactionData, TransactionEffects, TransactionKind,
};
use sui_types::move_package::disassemble_modules;
use sui_types::object::{Data, Object, ObjectRead, Owner};
use sui_types::sui_serde::{Base64, Encoding};

use sui_json::SuiJsonValue;

#[cfg(test)]
#[path = "unit_tests/gateway_types_tests.rs"]
mod gateway_types_tests;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct TransactionEffectsResponse {
    pub certificate: SuiCertifiedTransaction,
    pub effects: SuiTransactionEffects,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum TransactionResponse {
    EffectResponse(TransactionEffectsResponse),
    PublishResponse(PublishResponse),
    MergeCoinResponse(MergeCoinResponse),
    SplitCoinResponse(SplitCoinResponse),
}

impl TransactionResponse {
    pub fn to_publish_response(self) -> Result<PublishResponse, SuiError> {
        match self {
            TransactionResponse::PublishResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_merge_coin_response(self) -> Result<MergeCoinResponse, SuiError> {
        match self {
            TransactionResponse::MergeCoinResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_split_coin_response(self) -> Result<SplitCoinResponse, SuiError> {
        match self {
            TransactionResponse::SplitCoinResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_effect_response(self) -> Result<TransactionEffectsResponse, SuiError> {
        match self {
            TransactionResponse::EffectResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitCoinResponse {
    /// Certificate of the transaction
    pub certificate: SuiCertifiedTransaction,
    /// The updated original coin object after split
    pub updated_coin: SuiObject,
    /// All the newly created coin objects generated from the split
    pub new_coins: Vec<SuiObject>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiObject,
}

impl Display for SplitCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Certificate ----".bold())?;
        write!(writer, "{}", self.certificate)?;
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
pub struct MergeCoinResponse {
    /// Certificate of the transaction
    pub certificate: SuiCertifiedTransaction,
    /// The updated original coin object after merge
    pub updated_coin: SuiObject,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiObject,
}

impl Display for MergeCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Certificate ----".bold())?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "{}", "----- Merge Coin Results ----".bold())?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "Object")]
pub struct SuiObject {
    /// The meat of the object
    pub data: SuiData,
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

impl Display for SuiObject {
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

impl SuiObject {
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
                let move_struct = m
                    .to_move_struct(&layout.ok_or(SuiError::ObjectSerializationError {
                        error: "Layout is required to convert Move object to json".to_owned(),
                    })?)?
                    .into();

                if let SuiMoveStruct::WithTypes { type_, fields } = move_struct {
                    SuiData::MoveObject(SuiMoveObject {
                        type_,
                        fields: SuiMoveStruct::WithFields(fields),
                    })
                } else {
                    SuiData::MoveObject(SuiMoveObject {
                        type_: m.type_.to_string(),
                        fields: move_struct,
                    })
                }
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
pub enum SuiData {
    MoveObject(SuiMoveObject),
    Package(SuiMovePackage),
}

impl Display for SuiData {
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

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MoveObject")]
pub struct SuiMoveObject {
    #[serde(rename = "type")]
    pub type_: String,
    pub fields: SuiMoveStruct,
}

impl TryFrom<&SuiObject> for GasCoin {
    type Error = SuiError;
    fn try_from(object: &SuiObject) -> Result<Self, Self::Error> {
        match &object.data {
            SuiData::MoveObject(o) => {
                if GasCoin::type_().to_string() == o.type_ {
                    return GasCoin::try_from(&o.fields);
                }
            }
            SuiData::Package(_) => {}
        }

        return Err(SuiError::TypeError {
            error: format!(
                "Gas object type is not a gas coin: {:?}",
                object.data.type_()
            ),
        });
    }
}

impl TryFrom<&SuiMoveStruct> for GasCoin {
    type Error = SuiError;
    fn try_from(move_struct: &SuiMoveStruct) -> Result<Self, Self::Error> {
        match move_struct {
            SuiMoveStruct::WithFields(fields) | SuiMoveStruct::WithTypes { type_: _, fields } => {
                if let SuiMoveValue::Number(balance) = fields["balance"].clone() {
                    if let SuiMoveValue::VersionedID { id, version } = fields["id"].clone() {
                        return Ok(GasCoin::new(id, SequenceNumber::from(version), balance));
                    }
                }
            }
            _ => {}
        }
        return Err(SuiError::TypeError {
            error: format!("Struct is not a gas coin: {move_struct:?}"),
        });
    }
}

impl SuiData {
    pub fn try_as_package(&self) -> Option<&SuiMovePackage> {
        match self {
            SuiData::MoveObject(_) => None,
            SuiData::Package(p) => Some(p),
        }
    }
    pub fn type_(&self) -> Option<&str> {
        match self {
            SuiData::MoveObject(m) => Some(&m.type_),
            SuiData::Package(_) => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishResponse {
    /// Certificate of the transaction
    pub certificate: SuiCertifiedTransaction,
    /// The newly published package object reference.
    pub package: SuiObjectRef,
    /// List of Move objects created as part of running the module initializers in the package
    pub created_objects: Vec<SuiObject>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiObject,
}

impl Display for PublishResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "{}", "----- Certificate ----".bold())?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "{}", "----- Publish Results ----".bold())?;
        writeln!(
            writer,
            "The newly published package object ID: {:?}",
            self.package.object_id
        )?;
        if !self.created_objects.is_empty() {
            writeln!(
                writer,
                "List of objects created by running module initializers:\n"
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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", content = "details", rename = "ObjectRead")]
pub enum GetObjectDataResponse {
    Exists(SuiObject),
    NotExists(ObjectID),
    Deleted(SuiObjectRef),
}

impl GetObjectDataResponse {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> Result<&SuiObject, SuiError> {
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
    pub fn into_object(self) -> Result<SuiObject, SuiError> {
        match self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: id }),
            Self::Exists(o) => Ok(o),
        }
    }
}

impl TryFrom<ObjectRead> for GetObjectDataResponse {
    type Error = anyhow::Error;

    fn try_from(value: ObjectRead) -> Result<Self, Self::Error> {
        match value {
            ObjectRead::NotExists(id) => Ok(GetObjectDataResponse::NotExists(id)),
            ObjectRead::Exists(_, o, layout) => Ok(GetObjectDataResponse::Exists(
                SuiObject::try_from(o, layout)?,
            )),
            ObjectRead::Deleted(oref) => Ok(GetObjectDataResponse::Deleted(oref.into())),
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
    VersionedID { id: ObjectID, version: u64 },
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
            SuiMoveValue::VersionedID { id, version } => {
                write!(writer, "{id}[{version}]")?;
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
        "0x2::UTF8::String" | "0x1::ASCII::String" => {
            if let SuiMoveValue::Bytearray(bytes) = fields["bytes"].clone() {
                if let Ok(bytes) = bytes.to_vec() {
                    if let Ok(s) = String::from_utf8(bytes) {
                        return Some(SuiMoveValue::String(s));
                    }
                }
            }
        }
        "0x2::Url::Url" => return Some(fields["url"].clone()),
        "0x2::ID::ID" => {
            if let SuiMoveValue::Address(id) = fields["bytes"] {
                return Some(SuiMoveValue::Address(id));
            }
        }
        "0x2::ID::UniqueID" => {
            if let SuiMoveValue::Address(id) = fields["id"].clone() {
                return Some(SuiMoveValue::Address(id));
            }
        }
        "0x2::ID::VersionedID" => {
            if let SuiMoveValue::Address(address) = fields["id"].clone() {
                if let SuiMoveValue::Number(version) = fields["version"].clone() {
                    return Some(SuiMoveValue::VersionedID {
                        id: address.into(),
                        version,
                    });
                }
            }
        }
        "0x2::Balance::Balance" => {
            if let SuiMoveValue::Number(value) = fields["value"].clone() {
                return Some(SuiMoveValue::Number(value));
            }
        }
        "0x1::Option::Option" => {
            if let SuiMoveValue::Vector(values) = fields["vec"].clone() {
                return Some(SuiMoveValue::Option(Box::new(values.first().cloned())));
            }
        }
        _ => {}
    }
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
    sender: SuiAddress,
    gas_payment: SuiObjectRef,
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
    /// Initiate a coin transfer between addresses
    TransferCoin(SuiTransferCoin),
    /// Publish a new Move module
    Publish(SuiMovePackage),
    /// Call a function in a published Move module
    Call(SuiMoveCall),
    // .. more transaction types go here
}

impl Display for SuiTransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match &self {
            Self::TransferCoin(t) => {
                writeln!(writer, "Transaction Kind : Transfer")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                writeln!(writer, "Object ID : {}", t.object_ref.object_id)?;
                writeln!(writer, "Version : {:?}", t.object_ref.version)?;
                write!(
                    writer,
                    "Object Digest : {}",
                    Base64::encode(t.object_ref.digest)
                )?;
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
        }
        write!(f, "{}", writer)
    }
}

impl TryFrom<SingleTransactionKind> for SuiTransactionKind {
    type Error = anyhow::Error;

    fn try_from(tx: SingleTransactionKind) -> Result<Self, Self::Error> {
        Ok(match tx {
            SingleTransactionKind::TransferCoin(t) => Self::TransferCoin(SuiTransferCoin {
                recipient: t.recipient,
                object_ref: t.object_ref.into(),
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
                        CallArg::ImmOrOwnedObject((id, _, _)) => {
                            SuiJsonValue::new(Value::String(id.to_hex_literal()))
                        }
                        CallArg::SharedObject(id) => {
                            SuiJsonValue::new(Value::String(id.to_hex_literal()))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
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
#[serde(rename = "CertifiedTransaction", rename_all = "camelCase")]
pub struct SuiCertifiedTransaction {
    // This is a cache of an otherwise expensive to compute value.
    // DO NOT serialize or deserialize from the network or disk.
    pub transaction_digest: TransactionDigest,
    pub data: SuiTransactionData,
    /// tx_signature is signed by the transaction sender, applied on `data`.
    pub tx_signature: Signature,
    /// authority signature information, if available, is signed by an authority, applied on `data`.
    pub auth_sign_info: AuthorityQuorumSignInfo,
}

impl Display for SuiCertifiedTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Transaction Hash: {:?}", self.transaction_digest)?;
        writeln!(writer, "Transaction Signature: {:?}", self.tx_signature)?;
        writeln!(
            writer,
            "Signed Authorities : {:?}",
            self.auth_sign_info
                .signatures
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        )?;
        write!(writer, "{}", &self.data)?;
        write!(f, "{}", writer)
    }
}

impl TryFrom<CertifiedTransaction> for SuiCertifiedTransaction {
    type Error = anyhow::Error;

    fn try_from(cert: CertifiedTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction_digest: *cert.digest(),
            data: cert.data.try_into()?,
            tx_signature: cert.tx_signature,
            auth_sign_info: cert.auth_sign_info,
        })
    }
}

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransactionEffects", rename_all = "camelCase")]
pub struct SuiTransactionEffects {
    // The status of the execution
    pub status: SuiExecutionStatus,
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

impl From<TransactionEffects> for SuiTransactionEffects {
    fn from(effect: TransactionEffects) -> Self {
        Self {
            status: effect.status.into(),
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
                .iter()
                // TODO: figure out how to map the non-Move events
                .filter_map(|event| match event {
                    Event::MoveEvent { type_, contents } => Some(SuiEvent {
                        type_: type_.to_string(),
                        contents: contents.clone(),
                    }),
                    _ => None,
                })
                .collect(),
            dependencies: effect.dependencies,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "ExecutionStatus", rename_all = "camelCase", tag = "status")]
pub enum SuiExecutionStatus {
    // Gas used in the success case.
    Success {
        gas_cost: SuiGasCostSummary,
    },
    // Gas used in the failed case, and the error.
    Failure {
        gas_cost: SuiGasCostSummary,
        error: String,
    },
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
            ExecutionStatus::Success { gas_cost } => Self::Success {
                gas_cost: gas_cost.into(),
            },
            ExecutionStatus::Failure { gas_cost, error } => Self::Failure {
                gas_cost: gas_cost.into(),
                error: error.to_string(),
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

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "Event")]
// TODO: we need to reconstitute this for non Move events
pub struct SuiEvent {
    pub type_: String,
    pub contents: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransferCoin", rename_all = "camelCase")]
pub struct SuiTransferCoin {
    pub recipient: SuiAddress,
    pub object_ref: SuiObjectRef,
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
