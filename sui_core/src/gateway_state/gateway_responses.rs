// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use serde::ser::Error;
use serde::Serialize;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::coin::Coin;
use sui_types::error::SuiError;
use sui_types::gas_coin::GasCoin;
use sui_types::id::{UniqueID, VersionedID, ID};
use sui_types::json_schema;
use sui_types::messages::{CertifiedTransaction, TransactionEffects};
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, Object, ObjectRead, Owner};
use sui_types::readable_serde::encoding::{Base64, Encoding};
use sui_types::readable_serde::Readable;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct TransactionEffectsResponse {
    pub certificate: CertifiedTransaction,
    pub effects: TransactionEffects,
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

    pub fn to_effect_response(
        self,
    ) -> Result<(CertifiedTransaction, TransactionEffects), SuiError> {
        match self {
            TransactionResponse::EffectResponse(TransactionEffectsResponse {
                certificate,
                effects,
            }) => Ok((certificate, effects)),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct SplitCoinResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
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
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Split Coin Results ----")?;

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
pub struct MergeCoinResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
    /// The updated original coin object after merge
    pub updated_coin: SuiObject,
    /// The updated gas payment object after deducting payment
    pub updated_gas: SuiObject,
}

impl Display for MergeCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Merge Coin Results ----")?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
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
#[serde(rename_all = "camelCase")]
pub struct SuiObjectRef {
    /// Hex code as string representing the object id
    pub object_id: ObjectID,
    /// Object version.
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: String,
}

impl From<ObjectRef> for SuiObjectRef {
    fn from(oref: ObjectRef) -> Self {
        Self {
            object_id: oref.0,
            version: oref.1,
            digest: Base64::encode(oref.2),
        }
    }
}

impl Display for SuiObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let type_string = self
            .data
            .type_()
            .map_or("Move Package".to_owned(), |type_| type_.to_string());

        write!(
            f,
            "ID: {:?}\nVersion: {:?}\nOwner: {}\nType: {}",
            self.id(),
            self.version().value(),
            self.owner,
            type_string
        )
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
                let move_struct =
                    m.to_move_value(&layout.ok_or(SuiError::ObjectSerializationError {
                        error: "Layout is required to convert Move object to json".to_owned(),
                    })?)?;
                SuiData::MoveObject(SuiMoveObject {
                    type_: m.type_.to_string(),
                    contents: move_struct.into(),
                })
            }
            Data::Package(p) => SuiData::Package(p),
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
#[serde(tag = "dataType", rename_all = "camelCase")]
pub enum SuiData {
    MoveObject(SuiMoveObject),
    Package(MovePackage),
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
pub struct SuiMoveObject {
    #[serde(rename = "type")]
    pub type_: String,
    pub contents: SuiMoveValue,
}

impl TryFrom<&SuiObject> for GasCoin {
    type Error = SuiError;
    fn try_from(object: &SuiObject) -> Result<Self, Self::Error> {
        match &object.data {
            SuiData::MoveObject(o) => {
                if let SuiMoveValue::Coin(coin) = &o.contents {
                    return Ok(GasCoin(coin.clone()));
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

impl SuiData {
    pub fn try_as_package(&self) -> Option<&MovePackage> {
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
pub struct PublishResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
    /// The newly published package object reference.
    pub package: ObjectRef,
    /// List of Move objects created as part of running the module initializers in the package
    pub created_objects: Vec<Object>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}

impl Display for PublishResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Publish Results ----")?;
        writeln!(
            writer,
            "The newly published package object ID: {:?}",
            self.package.0
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

#[derive(Serialize, Clone, Debug)]
pub struct SwitchResponse {
    /// Active address
    pub address: Option<SuiAddress>,
    pub gateway: Option<String>,
}

impl Display for SwitchResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        if let Some(addr) = self.address {
            writeln!(writer, "Active address switched to {}", addr)?;
        }
        if let Some(gateway) = &self.gateway {
            writeln!(writer, "Active gateway switched to {}", gateway)?;
        }
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", content = "details")]
pub enum SuiObjectRead {
    Exists(SuiObject),
    NotExists(ObjectID),
    Deleted(ObjectRef),
}

impl SuiObjectRead {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> Result<&SuiObject, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(o) => Ok(o),
        }
    }

    /// Returns the object value if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn into_object(self) -> Result<SuiObject, SuiError> {
        match self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: id }),
            Self::Exists(o) => Ok(o),
        }
    }

    /// Returns the object ref if there is an object, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn reference(&self) -> Result<SuiObjectRef, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(o) => Ok(o.reference.clone()),
        }
    }
}

impl TryFrom<ObjectRead> for SuiObjectRead {
    type Error = anyhow::Error;

    fn try_from(value: ObjectRead) -> Result<Self, Self::Error> {
        match value {
            ObjectRead::NotExists(id) => Ok(SuiObjectRead::NotExists(id)),
            ObjectRead::Exists(_, o, layout) => {
                Ok(SuiObjectRead::Exists(SuiObject::try_from(o, layout)?))
            }
            ObjectRead::Deleted(oref) => Ok(SuiObjectRead::Deleted(oref)),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum SuiMoveValue {
    // Move base types
    U8(u8),
    U64(u64),
    U128(u128),
    Bool(bool),
    Address(ObjectID),
    Vector(Vec<SuiMoveValue>),
    Struct(SuiMoveStruct),
    Signer(SuiAddress),

    // Sui base types
    String(String),
    ID(ID),
    UniqueID(UniqueID),
    VersionedID(VersionedID),
    Balance(u64),
    ByteArray(
        #[schemars(with = "json_schema::Base64")]
        #[serde_as(as = "Readable<Base64, Bytes>")]
        Vec<u8>,
    ),
    Coin(Coin),
}

impl From<MoveValue> for SuiMoveValue {
    fn from(value: MoveValue) -> Self {
        match value {
            MoveValue::U8(value) => SuiMoveValue::U8(value),
            MoveValue::U64(value) => SuiMoveValue::U64(value),
            MoveValue::U128(value) => SuiMoveValue::U128(value),
            MoveValue::Bool(value) => SuiMoveValue::Bool(value),
            MoveValue::Address(value) => SuiMoveValue::Address(ObjectID::from(value)),
            MoveValue::Vector(value) => {
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
                    return SuiMoveValue::ByteArray(bytearray);
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
            MoveValue::Signer(value) => {
                SuiMoveValue::Signer(SuiAddress::from(ObjectID::from(value)))
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum SuiMoveStruct {
    Runtime(Vec<SuiMoveValue>),
    WithFields(Vec<(String, SuiMoveValue)>),
    WithTypes {
        #[serde(rename = "type")]
        type_: String,
        fields: BTreeMap<String, SuiMoveValue>,
    },
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
            if let SuiMoveValue::ByteArray(bytes) = fields["bytes"].clone() {
                if let Ok(s) = String::from_utf8(bytes) {
                    return Some(SuiMoveValue::String(s));
                }
            }
        }
        "0x2::Url::Url" => return Some(fields["url"].clone()),
        "0x2::ID::ID" => {
            if let SuiMoveValue::Address(id) = fields["bytes"] {
                return Some(SuiMoveValue::ID(ID { bytes: id }));
            }
        }
        "0x2::ID::UniqueID" => {
            if let SuiMoveValue::ID(id) = fields["id"].clone() {
                return Some(SuiMoveValue::UniqueID(UniqueID { id }));
            }
        }
        "0x2::ID::VersionedID" => {
            if let SuiMoveValue::UniqueID(id) = fields["id"].clone() {
                if let SuiMoveValue::U64(version) = fields["version"].clone() {
                    return Some(SuiMoveValue::VersionedID(VersionedID { id, version }));
                }
            }
        }
        "0x2::Balance::Balance" => {
            if let SuiMoveValue::U64(value) = fields["value"].clone() {
                return Some(SuiMoveValue::Balance(value));
            }
        }
        "0x2::Coin::Coin" => {
            if let SuiMoveValue::Balance(value) = fields["balance"].clone() {
                if let SuiMoveValue::VersionedID(id) = fields["id"].clone() {
                    return Some(SuiMoveValue::Coin(Coin { id, value }));
                }
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

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone)]
pub struct SuiMovePackage {
    disassembled: BTreeMap<String, Value>,
}
