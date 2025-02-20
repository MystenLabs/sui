// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use anyhow::anyhow;
use colored::Colorize;
use fastcrypto::encoding::Base64;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::annotated_value::{MoveStructLayout, MoveValue};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, ObjectRef, ObjectType, SequenceNumber, SuiAddress,
    TransactionDigest,
};
use sui_types::error::{
    ExecutionError, SuiError, SuiObjectResponseError, SuiResult, UserInputError, UserInputResult,
};
use sui_types::gas_coin::GasCoin;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::move_package::{MovePackage, TypeOrigin, UpgradeInfo};
use sui_types::object::{Data, MoveObject, Object, ObjectInner, ObjectRead, Owner};
use sui_types::sui_serde::BigInt;
use sui_types::sui_serde::SequenceNumber as AsSequenceNumber;
use sui_types::sui_serde::SuiStructTag;

use crate::{Page, SuiMoveStruct, SuiMoveValue};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, PartialEq, Eq)]
pub struct SuiObjectResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SuiObjectData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SuiObjectResponseError>,
}

impl SuiObjectResponse {
    pub fn new(data: Option<SuiObjectData>, error: Option<SuiObjectResponseError>) -> Self {
        Self { data, error }
    }

    pub fn new_with_data(data: SuiObjectData) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }

    pub fn new_with_error(error: SuiObjectResponseError) -> Self {
        Self {
            data: None,
            error: Some(error),
        }
    }
}

impl Ord for SuiObjectResponse {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.data, &other.data) {
            (Some(data), Some(data_2)) => {
                if data.object_id.cmp(&data_2.object_id).eq(&Ordering::Greater) {
                    return Ordering::Greater;
                } else if data.object_id.cmp(&data_2.object_id).eq(&Ordering::Less) {
                    return Ordering::Less;
                }
                Ordering::Equal
            }
            // In this ordering those with data will come before SuiObjectResponses that are errors.
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            // SuiObjectResponses that are errors are just considered equal.
            _ => Ordering::Equal,
        }
    }
}

impl PartialOrd for SuiObjectResponse {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl SuiObjectResponse {
    pub fn move_object_bcs(&self) -> Option<&Vec<u8>> {
        match &self.data {
            Some(SuiObjectData {
                bcs: Some(SuiRawData::MoveObject(obj)),
                ..
            }) => Some(&obj.bcs_bytes),
            _ => None,
        }
    }

    pub fn owner(&self) -> Option<Owner> {
        if let Some(data) = &self.data {
            return data.owner.clone();
        }
        None
    }

    pub fn object_id(&self) -> Result<ObjectID, anyhow::Error> {
        match (&self.data, &self.error) {
            (Some(obj_data), None) => Ok(obj_data.object_id),
            (None, Some(SuiObjectResponseError::NotExists { object_id })) => Ok(*object_id),
            (
                None,
                Some(SuiObjectResponseError::Deleted {
                    object_id,
                    version: _,
                    digest: _,
                }),
            ) => Ok(*object_id),
            _ => Err(anyhow!("Could not get object_id, something went wrong with SuiObjectResponse construction.")),
        }
    }

    pub fn object_ref_if_exists(&self) -> Option<ObjectRef> {
        match (&self.data, &self.error) {
            (Some(obj_data), None) => Some(obj_data.object_ref()),
            _ => None,
        }
    }
}

impl TryFrom<SuiObjectResponse> for ObjectInfo {
    type Error = anyhow::Error;

    fn try_from(value: SuiObjectResponse) -> Result<Self, Self::Error> {
        let SuiObjectData {
            object_id,
            version,
            digest,
            type_,
            owner,
            previous_transaction,
            ..
        } = value.into_object()?;

        Ok(ObjectInfo {
            object_id,
            version,
            digest,
            type_: type_.ok_or_else(|| anyhow!("Object type not found for object."))?,
            owner: owner.ok_or_else(|| anyhow!("Owner not found for object."))?,
            previous_transaction: previous_transaction
                .ok_or_else(|| anyhow!("Transaction digest not found for object."))?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
pub struct DisplayFieldsResponse {
    pub data: Option<BTreeMap<String, String>>,
    pub error: Option<SuiObjectResponseError>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ObjectData")]
pub struct SuiObjectData {
    pub object_id: ObjectID,
    /// Object version.
    #[schemars(with = "AsSequenceNumber")]
    #[serde_as(as = "AsSequenceNumber")]
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: ObjectDigest,
    /// The type of the object. Default to be None unless SuiObjectDataOptions.showType is set to true
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ObjectType>,
    // Default to be None because otherwise it will be repeated for the getOwnedObjects endpoint
    /// The owner of this object. Default to be None unless SuiObjectDataOptions.showOwner is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
    /// The digest of the transaction that created or last mutated this object. Default to be None unless
    /// SuiObjectDataOptions.showPreviousTransaction is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_transaction: Option<TransactionDigest>,
    /// The amount of SUI we would rebate if this object gets deleted.
    /// This number is re-calculated each time the object is mutated based on
    /// the present storage gas price.
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<BigInt<u64>>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_rebate: Option<u64>,
    /// The Display metadata for frontend UI rendering, default to be None unless SuiObjectDataOptions.showContent is set to true
    /// This can also be None if the struct type does not have Display defined
    /// See more details in <https://forums.sui.io/t/nft-object-display-proposal/4872>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayFieldsResponse>,
    /// Move object content or package content, default to be None unless SuiObjectDataOptions.showContent is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<SuiParsedData>,
    /// Move object content or package content in BCS, default to be None unless SuiObjectDataOptions.showBcs is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcs: Option<SuiRawData>,
}

impl SuiObjectData {
    pub fn object_ref(&self) -> ObjectRef {
        (self.object_id, self.version, self.digest)
    }

    pub fn object_type(&self) -> anyhow::Result<ObjectType> {
        self.type_
            .as_ref()
            .ok_or_else(|| anyhow!("type is missing for object {:?}", self.object_id))
            .cloned()
    }

    pub fn is_gas_coin(&self) -> bool {
        match self.type_.as_ref() {
            Some(ObjectType::Struct(ty)) if ty.is_gas_coin() => true,
            Some(_) => false,
            None => false,
        }
    }
}

impl Display for SuiObjectData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let type_ = if let Some(type_) = &self.type_ {
            type_.to_string()
        } else {
            "Unknown Type".into()
        };
        let mut writer = String::new();
        writeln!(
            writer,
            "{}",
            format!("----- {type_} ({}[{}]) -----", self.object_id, self.version).bold()
        )?;
        if let Some(owner) = &self.owner {
            writeln!(writer, "{}: {}", "Owner".bold().bright_black(), owner)?;
        }

        writeln!(
            writer,
            "{}: {}",
            "Version".bold().bright_black(),
            self.version
        )?;
        if let Some(storage_rebate) = self.storage_rebate {
            writeln!(
                writer,
                "{}: {}",
                "Storage Rebate".bold().bright_black(),
                storage_rebate
            )?;
        }

        if let Some(previous_transaction) = self.previous_transaction {
            writeln!(
                writer,
                "{}: {:?}",
                "Previous Transaction".bold().bright_black(),
                previous_transaction
            )?;
        }
        if let Some(content) = self.content.as_ref() {
            writeln!(writer, "{}", "----- Data -----".bold())?;
            write!(writer, "{}", content)?;
        }

        write!(f, "{}", writer)
    }
}

impl TryFrom<&SuiObjectData> for GasCoin {
    type Error = anyhow::Error;
    fn try_from(object: &SuiObjectData) -> Result<Self, Self::Error> {
        match &object
            .content
            .as_ref()
            .ok_or_else(|| anyhow!("Expect object content to not be empty"))?
        {
            SuiParsedData::MoveObject(o) => {
                if GasCoin::type_() == o.type_ {
                    return GasCoin::try_from(&o.fields);
                }
            }
            SuiParsedData::Package(_) => {}
        }

        Err(anyhow!(
            "Gas object type is not a gas coin: {:?}",
            object.type_
        ))
    }
}

impl TryFrom<&SuiMoveStruct> for GasCoin {
    type Error = anyhow::Error;
    fn try_from(move_struct: &SuiMoveStruct) -> Result<Self, Self::Error> {
        match move_struct {
            SuiMoveStruct::WithFields(fields) | SuiMoveStruct::WithTypes { type_: _, fields } => {
                if let Some(SuiMoveValue::String(balance)) = fields.get("balance") {
                    if let Ok(balance) = balance.parse::<u64>() {
                        if let Some(SuiMoveValue::UID { id }) = fields.get("id") {
                            return Ok(GasCoin::new(*id, balance));
                        }
                    }
                }
            }
            _ => {}
        }
        Err(anyhow!("Struct is not a gas coin: {move_struct:?}"))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase", rename = "ObjectDataOptions", default)]
pub struct SuiObjectDataOptions {
    /// Whether to show the type of the object. Default to be False
    pub show_type: bool,
    /// Whether to show the owner of the object. Default to be False
    pub show_owner: bool,
    /// Whether to show the previous transaction digest of the object. Default to be False
    pub show_previous_transaction: bool,
    /// Whether to show the Display metadata of the object for frontend rendering. Default to be False
    pub show_display: bool,
    /// Whether to show the content(i.e., package content or Move struct content) of the object.
    /// Default to be False
    pub show_content: bool,
    /// Whether to show the content in BCS format. Default to be False
    pub show_bcs: bool,
    /// Whether to show the storage rebate of the object. Default to be False
    pub show_storage_rebate: bool,
}

impl SuiObjectDataOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// return BCS data and all other metadata such as storage rebate
    pub fn bcs_lossless() -> Self {
        Self {
            show_bcs: true,
            show_type: true,
            show_owner: true,
            show_previous_transaction: true,
            show_display: false,
            show_content: false,
            show_storage_rebate: true,
        }
    }

    /// return full content except bcs
    pub fn full_content() -> Self {
        Self {
            show_bcs: false,
            show_type: true,
            show_owner: true,
            show_previous_transaction: true,
            show_display: false,
            show_content: true,
            show_storage_rebate: true,
        }
    }

    pub fn with_content(mut self) -> Self {
        self.show_content = true;
        self
    }

    pub fn with_owner(mut self) -> Self {
        self.show_owner = true;
        self
    }

    pub fn with_type(mut self) -> Self {
        self.show_type = true;
        self
    }

    pub fn with_display(mut self) -> Self {
        self.show_display = true;
        self
    }

    pub fn with_bcs(mut self) -> Self {
        self.show_bcs = true;
        self
    }

    pub fn with_previous_transaction(mut self) -> Self {
        self.show_previous_transaction = true;
        self
    }

    pub fn is_not_in_object_info(&self) -> bool {
        self.show_bcs || self.show_content || self.show_display || self.show_storage_rebate
    }
}

impl TryFrom<(ObjectRead, SuiObjectDataOptions)> for SuiObjectResponse {
    type Error = anyhow::Error;

    fn try_from(
        (object_read, options): (ObjectRead, SuiObjectDataOptions),
    ) -> Result<Self, Self::Error> {
        match object_read {
            ObjectRead::NotExists(id) => Ok(SuiObjectResponse::new_with_error(
                SuiObjectResponseError::NotExists { object_id: id },
            )),
            ObjectRead::Exists(object_ref, o, layout) => {
                let data = (object_ref, o, layout, options).try_into()?;
                Ok(SuiObjectResponse::new_with_data(data))
            }
            ObjectRead::Deleted((object_id, version, digest)) => Ok(
                SuiObjectResponse::new_with_error(SuiObjectResponseError::Deleted {
                    object_id,
                    version,
                    digest,
                }),
            ),
        }
    }
}

impl TryFrom<(ObjectInfo, SuiObjectDataOptions)> for SuiObjectResponse {
    type Error = anyhow::Error;

    fn try_from(
        (object_info, options): (ObjectInfo, SuiObjectDataOptions),
    ) -> Result<Self, Self::Error> {
        let SuiObjectDataOptions {
            show_type,
            show_owner,
            show_previous_transaction,
            ..
        } = options;

        Ok(Self::new_with_data(SuiObjectData {
            object_id: object_info.object_id,
            version: object_info.version,
            digest: object_info.digest,
            type_: show_type.then_some(object_info.type_),
            owner: show_owner.then_some(object_info.owner),
            previous_transaction: show_previous_transaction
                .then_some(object_info.previous_transaction),
            storage_rebate: None,
            display: None,
            content: None,
            bcs: None,
        }))
    }
}

impl
    TryFrom<(
        ObjectRef,
        Object,
        Option<MoveStructLayout>,
        SuiObjectDataOptions,
    )> for SuiObjectData
{
    type Error = anyhow::Error;

    fn try_from(
        (object_ref, o, layout, options): (
            ObjectRef,
            Object,
            Option<MoveStructLayout>,
            SuiObjectDataOptions,
        ),
    ) -> Result<Self, Self::Error> {
        let SuiObjectDataOptions {
            show_type,
            show_owner,
            show_previous_transaction,
            show_content,
            show_bcs,
            show_storage_rebate,
            ..
        } = options;

        let (object_id, version, digest) = object_ref;
        let type_ = if show_type {
            Some(Into::<ObjectType>::into(&o))
        } else {
            None
        };

        let bcs: Option<SuiRawData> = if show_bcs {
            let data = match o.data.clone() {
                Data::Move(m) => {
                    let layout = layout.clone().ok_or_else(|| {
                        anyhow!("Layout is required to convert Move object to json")
                    })?;
                    SuiRawData::try_from_object(m, layout)?
                }
                Data::Package(p) => SuiRawData::try_from_package(p)
                    .map_err(|e| anyhow!("Error getting raw data from package: {e:#?}"))?,
            };
            Some(data)
        } else {
            None
        };

        let o = o.into_inner();

        let content: Option<SuiParsedData> = if show_content {
            let data = match o.data {
                Data::Move(m) => {
                    let layout = layout.ok_or_else(|| {
                        anyhow!("Layout is required to convert Move object to json")
                    })?;
                    SuiParsedData::try_from_object(m, layout)?
                }
                Data::Package(p) => SuiParsedData::try_from_package(p)?,
            };
            Some(data)
        } else {
            None
        };

        Ok(SuiObjectData {
            object_id,
            version,
            digest,
            type_,
            owner: if show_owner { Some(o.owner) } else { None },
            storage_rebate: if show_storage_rebate {
                Some(o.storage_rebate)
            } else {
                None
            },
            previous_transaction: if show_previous_transaction {
                Some(o.previous_transaction)
            } else {
                None
            },
            content,
            bcs,
            display: None,
        })
    }
}

impl
    TryFrom<(
        ObjectRef,
        Object,
        Option<MoveStructLayout>,
        SuiObjectDataOptions,
        Option<DisplayFieldsResponse>,
    )> for SuiObjectData
{
    type Error = anyhow::Error;

    fn try_from(
        (object_ref, o, layout, options, display_fields): (
            ObjectRef,
            Object,
            Option<MoveStructLayout>,
            SuiObjectDataOptions,
            Option<DisplayFieldsResponse>,
        ),
    ) -> Result<Self, Self::Error> {
        let show_display = options.show_display;
        let mut data: SuiObjectData = (object_ref, o, layout, options).try_into()?;
        if show_display {
            data.display = display_fields;
        }
        Ok(data)
    }
}

impl SuiObjectResponse {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> Result<&SuiObjectData, SuiObjectResponseError> {
        if let Some(data) = &self.data {
            Ok(data)
        } else if let Some(error) = &self.error {
            Err(error.clone())
        } else {
            // We really shouldn't reach this code block since either data, or error field should always be filled.
            Err(SuiObjectResponseError::Unknown)
        }
    }

    /// Returns the object value if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn into_object(self) -> Result<SuiObjectData, SuiObjectResponseError> {
        match self.object() {
            Ok(data) => Ok(data.clone()),
            Err(error) => Err(error),
        }
    }
}

impl TryInto<Object> for SuiObjectData {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Object, Self::Error> {
        let protocol_config = ProtocolConfig::get_for_min_version();
        let data = match self.bcs {
            Some(SuiRawData::MoveObject(o)) => Data::Move(unsafe {
                MoveObject::new_from_execution(
                    o.type_().clone().into(),
                    o.has_public_transfer,
                    o.version,
                    o.bcs_bytes,
                    &protocol_config,
                )?
            }),
            Some(SuiRawData::Package(p)) => Data::Package(MovePackage::new(
                p.id,
                self.version,
                p.module_map,
                protocol_config.max_move_package_size(),
                p.type_origin_table,
                p.linkage_table,
            )?),
            _ => Err(anyhow!(
                "BCS data is required to convert SuiObjectData to Object"
            ))?,
        };
        Ok(ObjectInner {
            data,
            owner: self
                .owner
                .ok_or_else(|| anyhow!("Owner is required to convert SuiObjectData to Object"))?,
            previous_transaction: self.previous_transaction.ok_or_else(|| {
                anyhow!("previous_transaction is required to convert SuiObjectData to Object")
            })?,
            storage_rebate: self.storage_rebate.ok_or_else(|| {
                anyhow!("storage_rebate is required to convert SuiObjectData to Object")
            })?,
        }
        .into())
    }
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

impl Display for SuiObjectRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Object ID: {}, version: {}, digest: {}",
            self.object_id, self.version, self.digest
        )
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

pub trait SuiData: Sized {
    type ObjectType;
    type PackageType;
    fn try_from_object(object: MoveObject, layout: MoveStructLayout)
        -> Result<Self, anyhow::Error>;
    fn try_from_package(package: MovePackage) -> Result<Self, anyhow::Error>;
    fn try_as_move(&self) -> Option<&Self::ObjectType>;
    fn try_into_move(self) -> Option<Self::ObjectType>;
    fn try_as_package(&self) -> Option<&Self::PackageType>;
    fn type_(&self) -> Option<&StructTag>;
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(tag = "dataType", rename_all = "camelCase", rename = "RawData")]
pub enum SuiRawData {
    // Manually handle generic schema generation
    MoveObject(SuiRawMoveObject),
    Package(SuiRawMovePackage),
}

impl SuiData for SuiRawData {
    type ObjectType = SuiRawMoveObject;
    type PackageType = SuiRawMovePackage;

    fn try_from_object(object: MoveObject, _: MoveStructLayout) -> Result<Self, anyhow::Error> {
        Ok(Self::MoveObject(object.into()))
    }

    fn try_from_package(package: MovePackage) -> Result<Self, anyhow::Error> {
        Ok(Self::Package(package.into()))
    }

    fn try_as_move(&self) -> Option<&Self::ObjectType> {
        match self {
            Self::MoveObject(o) => Some(o),
            Self::Package(_) => None,
        }
    }

    fn try_into_move(self) -> Option<Self::ObjectType> {
        match self {
            Self::MoveObject(o) => Some(o),
            Self::Package(_) => None,
        }
    }

    fn try_as_package(&self) -> Option<&Self::PackageType> {
        match self {
            Self::MoveObject(_) => None,
            Self::Package(p) => Some(p),
        }
    }

    fn type_(&self) -> Option<&StructTag> {
        match self {
            Self::MoveObject(o) => Some(&o.type_),
            Self::Package(_) => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(tag = "dataType", rename_all = "camelCase", rename = "Data")]
pub enum SuiParsedData {
    // Manually handle generic schema generation
    MoveObject(SuiParsedMoveObject),
    Package(SuiMovePackage),
}

impl SuiData for SuiParsedData {
    type ObjectType = SuiParsedMoveObject;
    type PackageType = SuiMovePackage;

    fn try_from_object(
        object: MoveObject,
        layout: MoveStructLayout,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self::MoveObject(SuiParsedMoveObject::try_from_layout(
            object, layout,
        )?))
    }

    fn try_from_package(package: MovePackage) -> Result<Self, anyhow::Error> {
        let mut disassembled = BTreeMap::new();
        for bytecode in package.serialized_module_map().values() {
            // this function is only from JSON RPC - it is OK to deserialize with max Move binary
            // version
            let module = move_binary_format::CompiledModule::deserialize_with_defaults(bytecode)
                .map_err(|error| SuiError::ModuleDeserializationFailure {
                    error: error.to_string(),
                })?;
            let d = move_disassembler::disassembler::Disassembler::from_module_with_max_size(
                &module,
                move_ir_types::location::Spanned::unsafe_no_loc(()).loc,
                *sui_types::move_package::MAX_DISASSEMBLED_MODULE_SIZE,
            )
            .map_err(|e| SuiError::ObjectSerializationError {
                error: e.to_string(),
            })?;
            let bytecode_str = d
                .disassemble()
                .map_err(|e| SuiError::ObjectSerializationError {
                    error: e.to_string(),
                })?;
            disassembled.insert(module.name().to_string(), Value::String(bytecode_str));
        }

        Ok(Self::Package(SuiMovePackage { disassembled }))
    }

    fn try_as_move(&self) -> Option<&Self::ObjectType> {
        match self {
            Self::MoveObject(o) => Some(o),
            Self::Package(_) => None,
        }
    }

    fn try_into_move(self) -> Option<Self::ObjectType> {
        match self {
            Self::MoveObject(o) => Some(o),
            Self::Package(_) => None,
        }
    }

    fn try_as_package(&self) -> Option<&Self::PackageType> {
        match self {
            Self::MoveObject(_) => None,
            Self::Package(p) => Some(p),
        }
    }

    fn type_(&self) -> Option<&StructTag> {
        match self {
            Self::MoveObject(o) => Some(&o.type_),
            Self::Package(_) => None,
        }
    }
}

impl Display for SuiParsedData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match self {
            SuiParsedData::MoveObject(o) => {
                writeln!(writer, "{}: {}", "type".bold().bright_black(), o.type_)?;
                write!(writer, "{}", &o.fields)?;
            }
            SuiParsedData::Package(p) => {
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

impl SuiParsedData {
    pub fn try_from_object_read(object_read: ObjectRead) -> Result<Self, anyhow::Error> {
        match object_read {
            ObjectRead::NotExists(id) => Err(anyhow::anyhow!("Object {} does not exist", id)),
            ObjectRead::Exists(_object_ref, o, layout) => {
                let data = match o.into_inner().data {
                    Data::Move(m) => {
                        let layout = layout.ok_or_else(|| {
                            anyhow!("Layout is required to convert Move object to json")
                        })?;
                        SuiParsedData::try_from_object(m, layout)?
                    }
                    Data::Package(p) => SuiParsedData::try_from_package(p)?,
                };
                Ok(data)
            }
            ObjectRead::Deleted((object_id, version, digest)) => Err(anyhow::anyhow!(
                "Object {} was deleted at version {} with digest {}",
                object_id,
                version,
                digest
            )),
        }
    }
}

pub trait SuiMoveObject: Sized {
    fn try_from_layout(object: MoveObject, layout: MoveStructLayout)
        -> Result<Self, anyhow::Error>;

    fn try_from(o: MoveObject, resolver: &impl GetModule) -> Result<Self, anyhow::Error> {
        let layout = o.get_layout(resolver)?;
        Self::try_from_layout(o, layout)
    }

    fn type_(&self) -> &StructTag;
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MoveObject", rename_all = "camelCase")]
pub struct SuiParsedMoveObject {
    #[serde(rename = "type")]
    #[serde_as(as = "SuiStructTag")]
    #[schemars(with = "String")]
    pub type_: StructTag,
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
                    type_: object.type_().clone().into(),
                    has_public_transfer: object.has_public_transfer(),
                    fields: move_struct,
                }
            },
        )
    }

    fn type_(&self) -> &StructTag {
        &self.type_
    }
}

impl SuiParsedMoveObject {
    pub fn try_from_object_read(object_read: ObjectRead) -> Result<Self, anyhow::Error> {
        let parsed_data = SuiParsedData::try_from_object_read(object_read)?;
        match parsed_data {
            SuiParsedData::MoveObject(o) => Ok(o),
            SuiParsedData::Package(_) => Err(anyhow::anyhow!("Object is not a Move object")),
        }
    }
}

pub fn type_and_fields_from_move_event_data(
    event_data: MoveValue,
) -> SuiResult<(StructTag, serde_json::Value)> {
    match event_data.into() {
        SuiMoveValue::Struct(move_struct) => match &move_struct {
            SuiMoveStruct::WithTypes { type_, .. } => {
                Ok((type_.clone(), move_struct.clone().to_json_value()))
            }
            _ => Err(SuiError::ObjectDeserializationError {
                error: "Found non-type SuiMoveStruct in MoveValue event".to_string(),
            }),
        },
        SuiMoveValue::Variant(v) => Ok((v.type_.clone(), v.clone().to_json_value())),
        SuiMoveValue::Vector(_)
        | SuiMoveValue::Number(_)
        | SuiMoveValue::Bool(_)
        | SuiMoveValue::Address(_)
        | SuiMoveValue::String(_)
        | SuiMoveValue::UID { .. }
        | SuiMoveValue::Option(_) => Err(SuiError::ObjectDeserializationError {
            error: "Invalid MoveValue event type -- this should not be possible".to_string(),
        }),
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "RawMoveObject", rename_all = "camelCase")]
pub struct SuiRawMoveObject {
    #[schemars(with = "String")]
    #[serde(rename = "type")]
    #[serde_as(as = "SuiStructTag")]
    pub type_: StructTag,
    pub has_public_transfer: bool,
    pub version: SequenceNumber,
    #[serde_as(as = "Base64")]
    #[schemars(with = "Base64")]
    pub bcs_bytes: Vec<u8>,
}

impl From<MoveObject> for SuiRawMoveObject {
    fn from(o: MoveObject) -> Self {
        Self {
            type_: o.type_().clone().into(),
            has_public_transfer: o.has_public_transfer(),
            version: o.version(),
            bcs_bytes: o.into_contents(),
        }
    }
}

impl SuiMoveObject for SuiRawMoveObject {
    fn try_from_layout(
        object: MoveObject,
        _layout: MoveStructLayout,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            type_: object.type_().clone().into(),
            has_public_transfer: object.has_public_transfer(),
            version: object.version(),
            bcs_bytes: object.into_contents(),
        })
    }

    fn type_(&self) -> &StructTag {
        &self.type_
    }
}

impl SuiRawMoveObject {
    pub fn deserialize<'a, T: Deserialize<'a>>(&'a self) -> Result<T, anyhow::Error> {
        Ok(bcs::from_bytes(self.bcs_bytes.as_slice())?)
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "RawMovePackage", rename_all = "camelCase")]
pub struct SuiRawMovePackage {
    pub id: ObjectID,
    pub version: SequenceNumber,
    #[schemars(with = "BTreeMap<String, Base64>")]
    #[serde_as(as = "BTreeMap<_, Base64>")]
    pub module_map: BTreeMap<String, Vec<u8>>,
    pub type_origin_table: Vec<TypeOrigin>,
    pub linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
}

impl From<MovePackage> for SuiRawMovePackage {
    fn from(p: MovePackage) -> Self {
        Self {
            id: p.id(),
            version: p.version(),
            module_map: p.serialized_module_map().clone(),
            type_origin_table: p.type_origin_table().clone(),
            linkage_table: p.linkage_table().clone(),
        }
    }
}

impl SuiRawMovePackage {
    pub fn to_move_package(
        &self,
        max_move_package_size: u64,
    ) -> Result<MovePackage, ExecutionError> {
        MovePackage::new(
            self.id,
            self.version,
            self.module_map.clone(),
            max_move_package_size,
            self.type_origin_table.clone(),
            self.linkage_table.clone(),
        )
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, PartialEq, Eq)]
#[serde(tag = "status", content = "details", rename = "ObjectRead")]
pub enum SuiPastObjectResponse {
    /// The object exists and is found with this version
    VersionFound(SuiObjectData),
    /// The object does not exist
    ObjectNotExists(ObjectID),
    /// The object is found to be deleted with this version
    ObjectDeleted(SuiObjectRef),
    /// The object exists but not found with this version
    VersionNotFound(ObjectID, SequenceNumber),
    /// The asked object version is higher than the latest
    VersionTooHigh {
        object_id: ObjectID,
        asked_version: SequenceNumber,
        latest_version: SequenceNumber,
    },
}

impl SuiPastObjectResponse {
    /// Returns a reference to the object if there is any, otherwise an Err
    pub fn object(&self) -> UserInputResult<&SuiObjectData> {
        match &self {
            Self::ObjectDeleted(oref) => Err(UserInputError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::ObjectNotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: *id,
                version: None,
            }),
            Self::VersionFound(o) => Ok(o),
            Self::VersionNotFound(id, seq_num) => Err(UserInputError::ObjectNotFound {
                object_id: *id,
                version: Some(*seq_num),
            }),
            Self::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            } => Err(UserInputError::ObjectSequenceNumberTooHigh {
                object_id: *object_id,
                asked_version: *asked_version,
                latest_version: *latest_version,
            }),
        }
    }

    /// Returns the object value if there is any, otherwise an Err
    pub fn into_object(self) -> UserInputResult<SuiObjectData> {
        match self {
            Self::ObjectDeleted(oref) => Err(UserInputError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::ObjectNotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: id,
                version: None,
            }),
            Self::VersionFound(o) => Ok(o),
            Self::VersionNotFound(object_id, version) => Err(UserInputError::ObjectNotFound {
                object_id,
                version: Some(version),
            }),
            Self::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            } => Err(UserInputError::ObjectSequenceNumberTooHigh {
                object_id,
                asked_version,
                latest_version,
            }),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MovePackage", rename_all = "camelCase")]
pub struct SuiMovePackage {
    pub disassembled: BTreeMap<String, Value>,
}

pub type QueryObjectsPage = Page<SuiObjectResponse, CheckpointedObjectID>;
pub type ObjectsPage = Page<SuiObjectResponse, ObjectID>;

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Copy, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointedObjectID {
    pub object_id: ObjectID,
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<BigInt<u64>>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_checkpoint: Option<CheckpointSequenceNumber>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "GetPastObjectRequest", rename_all = "camelCase")]
pub struct SuiGetPastObjectRequest {
    /// the ID of the queried object
    pub object_id: ObjectID,
    /// the version of the queried object.
    #[schemars(with = "AsSequenceNumber")]
    #[serde_as(as = "AsSequenceNumber")]
    pub version: SequenceNumber,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum SuiObjectDataFilter {
    MatchAll(Vec<SuiObjectDataFilter>),
    MatchAny(Vec<SuiObjectDataFilter>),
    MatchNone(Vec<SuiObjectDataFilter>),
    /// Query by type a specified Package.
    Package(ObjectID),
    /// Query by type a specified Move module.
    MoveModule {
        /// the Move package ID
        package: ObjectID,
        /// the module name
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        module: Identifier,
    },
    /// Query by type
    StructType(
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        StructTag,
    ),
    AddressOwner(SuiAddress),
    ObjectOwner(ObjectID),
    ObjectId(ObjectID),
    // allow querying for multiple object ids
    ObjectIds(Vec<ObjectID>),
    Version(
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        u64,
    ),
}

impl SuiObjectDataFilter {
    pub fn gas_coin() -> Self {
        Self::StructType(GasCoin::type_())
    }

    pub fn and(self, other: Self) -> Self {
        Self::MatchAll(vec![self, other])
    }
    pub fn or(self, other: Self) -> Self {
        Self::MatchAny(vec![self, other])
    }
    pub fn not(self, other: Self) -> Self {
        Self::MatchNone(vec![self, other])
    }

    pub fn matches(&self, object: &ObjectInfo) -> bool {
        match self {
            SuiObjectDataFilter::MatchAll(filters) => !filters.iter().any(|f| !f.matches(object)),
            SuiObjectDataFilter::MatchAny(filters) => filters.iter().any(|f| f.matches(object)),
            SuiObjectDataFilter::MatchNone(filters) => !filters.iter().any(|f| f.matches(object)),
            SuiObjectDataFilter::StructType(s) => {
                let obj_tag: StructTag = match &object.type_ {
                    ObjectType::Package => return false,
                    ObjectType::Struct(s) => s.clone().into(),
                };
                // If people do not provide type_params, we will match all type_params
                // e.g. `0x2::coin::Coin` can match `0x2::coin::Coin<0x2::sui::SUI>`
                if !s.type_params.is_empty() && s.type_params != obj_tag.type_params {
                    false
                } else {
                    obj_tag.address == s.address
                        && obj_tag.module == s.module
                        && obj_tag.name == s.name
                }
            }
            SuiObjectDataFilter::MoveModule { package, module } => {
                matches!(&object.type_, ObjectType::Struct(s) if &ObjectID::from(s.address()) == package
                        && s.module() == module.as_ident_str())
            }
            SuiObjectDataFilter::Package(p) => {
                matches!(&object.type_, ObjectType::Struct(s) if &ObjectID::from(s.address()) == p)
            }
            SuiObjectDataFilter::AddressOwner(a) => {
                matches!(object.owner, Owner::AddressOwner(addr) if &addr == a)
            }
            SuiObjectDataFilter::ObjectOwner(o) => {
                matches!(object.owner, Owner::ObjectOwner(addr) if addr == SuiAddress::from(*o))
            }
            SuiObjectDataFilter::ObjectId(id) => &object.object_id == id,
            SuiObjectDataFilter::ObjectIds(ids) => ids.contains(&object.object_id),
            SuiObjectDataFilter::Version(v) => object.version.value() == *v,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", rename = "ObjectResponseQuery", default)]
pub struct SuiObjectResponseQuery {
    /// If None, no filter will be applied
    pub filter: Option<SuiObjectDataFilter>,
    /// config which fields to include in the response, by default only digest is included
    pub options: Option<SuiObjectDataOptions>,
}

impl SuiObjectResponseQuery {
    pub fn new(filter: Option<SuiObjectDataFilter>, options: Option<SuiObjectDataOptions>) -> Self {
        Self { filter, options }
    }

    pub fn new_with_filter(filter: SuiObjectDataFilter) -> Self {
        Self {
            filter: Some(filter),
            options: None,
        }
    }

    pub fn new_with_options(options: SuiObjectDataOptions) -> Self {
        Self {
            filter: None,
            options: Some(options),
        }
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum ZkLoginIntentScope {
    TransactionData,
    PersonalMessage,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ZkLoginVerifyResult")]
pub struct ZkLoginVerifyResult {
    /// The boolean result of the verification. If true, errors should be empty.
    pub success: bool,
    /// The errors field captures any verification error
    pub errors: Vec<String>,
}
