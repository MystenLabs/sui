// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use anyhow::anyhow;
use colored::Colorize;
use fastcrypto::encoding::Base64;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_with::serde_as;

use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, ObjectRef, ObjectType, SequenceNumber, TransactionDigest,
};
use sui_types::error::{UserInputError, UserInputResult};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::MoveModulePublish;
use sui_types::move_package::{disassemble_modules, MovePackage};
use sui_types::object::{Data, MoveObject, Object, ObjectFormatOptions, ObjectRead, Owner};
use sui_types::parse_sui_struct_tag;

use crate::{SuiMoveStruct, SuiMoveValue};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "status", content = "details", rename = "ObjectRead")]
pub enum SuiObjectResponse {
    Exists(SuiObjectData),
    NotExists(ObjectID),
    Deleted(SuiObjectRef),
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ObjectData")]
pub struct SuiObjectData {
    pub object_id: ObjectID,
    /// Object version.
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: ObjectDigest,
    /// The type of the object. Default to be None unless SuiObjectDataOptions.showType is set to true
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    // Default to be None because otherwise it will be repeated for the getObjectsOwnedByAddress endpoint
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_rebate: Option<u64>,
    /// The Display metadata for frontend UI rendering, default to be None unless SuiObjectDataOptions.showContent is set to true
    /// This can also be None if the struct type does not have Display defined
    /// See more details in <https://forums.sui.io/t/nft-object-display-proposal/4872>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<BTreeMap<String, String>>,
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
            .ok_or_else(|| anyhow!("type is missing for object {:?}", self.object_id))?
            .parse()
    }
}

impl Display for SuiObjectData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let type_ = self.type_.clone().unwrap_or_default();
        let mut writer = String::new();
        writeln!(
            writer,
            "{}",
            format!("----- {type_} ({}[{}]) -----", self.object_id, self.version).bold()
        )?;
        if let Some(owner) = self.owner {
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
                if GasCoin::type_().to_string() == o.type_ {
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
#[serde(rename_all = "camelCase", rename = "ObjectContentOptions", default)]
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
}

impl TryFrom<(ObjectRead, SuiObjectDataOptions)> for SuiObjectResponse {
    type Error = anyhow::Error;

    fn try_from(
        (object_read, options): (ObjectRead, SuiObjectDataOptions),
    ) -> Result<Self, Self::Error> {
        match object_read {
            ObjectRead::NotExists(id) => Ok(Self::NotExists(id)),
            ObjectRead::Exists(object_ref, o, layout) => {
                Ok(Self::Exists((object_ref, o, layout, options).try_into()?))
            }
            ObjectRead::Deleted(oref) => Ok(Self::Deleted(oref.into())),
        }
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
            Some(Into::<ObjectType>::into(&o).to_string())
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
                Data::Package(p) => SuiRawData::try_from_package(p)?,
            };
            Some(data)
        } else {
            None
        };

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
        Option<BTreeMap<String, String>>,
    )> for SuiObjectData
{
    type Error = anyhow::Error;

    fn try_from(
        (object_ref, o, layout, options, display_fields): (
            ObjectRef,
            Object,
            Option<MoveStructLayout>,
            SuiObjectDataOptions,
            Option<BTreeMap<String, String>>,
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
    pub fn object(&self) -> UserInputResult<&SuiObjectData> {
        match &self {
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::NotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: *id,
                version: None,
            }),
            Self::Exists(o) => Ok(o),
        }
    }

    /// Returns the object value if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn into_object(self) -> UserInputResult<SuiObjectData> {
        match self {
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted {
                object_ref: oref.to_object_ref(),
            }),
            Self::NotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: id,
                version: None,
            }),
            Self::Exists(o) => Ok(o),
        }
    }
}

impl TryInto<Object> for SuiObjectData {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Object, Self::Error> {
        let protocol_config = ProtocolConfig::get_for_min_version();
        let data = match self.bcs {
            Some(SuiRawData::MoveObject(o)) => {
                let struct_tag = parse_sui_struct_tag(o.type_())?;
                Data::Move(unsafe {
                    MoveObject::new_from_execution(
                        struct_tag,
                        o.has_public_transfer,
                        o.version,
                        o.bcs_bytes,
                        &protocol_config,
                    )?
                })
            }
            Some(SuiRawData::Package(p)) => Data::Package(MovePackage::new(
                p.id,
                self.version,
                &p.module_map,
                protocol_config.max_move_package_size(),
            )?),
            _ => Err(anyhow!(
                "BCS data is required to convert SuiObjectData to Object"
            ))?,
        };
        Ok(Object {
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
        })
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
    fn try_as_package(&self) -> Option<&Self::PackageType>;
    fn type_(&self) -> Option<&str>;
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

    fn try_as_package(&self) -> Option<&Self::PackageType> {
        match self {
            Self::MoveObject(_) => None,
            Self::Package(p) => Some(p),
        }
    }

    fn type_(&self) -> Option<&str> {
        match self {
            Self::MoveObject(o) => Some(o.type_.as_ref()),
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
        Ok(Self::Package(SuiMovePackage {
            disassembled: package.disassemble()?,
        }))
    }

    fn try_as_move(&self) -> Option<&Self::ObjectType> {
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

    fn type_(&self) -> Option<&str> {
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
#[serde(rename = "MoveObject", rename_all = "camelCase")]
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

pub fn type_and_fields_from_move_struct(
    type_: &StructTag,
    move_struct: MoveStruct,
) -> (String, SuiMoveStruct) {
    match move_struct.into() {
        SuiMoveStruct::WithTypes { type_, fields } => (type_, SuiMoveStruct::WithFields(fields)),
        fields => (type_.to_string(), fields),
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "RawMoveObject", rename_all = "camelCase")]
pub struct SuiRawMoveObject {
    #[serde(rename = "type")]
    pub type_: String,
    pub has_public_transfer: bool,
    pub version: SequenceNumber,
    #[serde_as(as = "Base64")]
    #[schemars(with = "Base64")]
    pub bcs_bytes: Vec<u8>,
}

impl From<MoveObject> for SuiRawMoveObject {
    fn from(o: MoveObject) -> Self {
        Self {
            type_: o.type_.to_string(),
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
            type_: object.type_.to_string(),
            has_public_transfer: object.has_public_transfer(),
            version: object.version(),
            bcs_bytes: object.into_contents(),
        })
    }

    fn type_(&self) -> &str {
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
    #[schemars(with = "BTreeMap<String, Base64>")]
    #[serde_as(as = "BTreeMap<_, Base64>")]
    pub module_map: BTreeMap<String, Vec<u8>>,
}

impl From<MovePackage> for SuiRawMovePackage {
    fn from(p: MovePackage) -> Self {
        Self {
            id: p.id(),
            module_map: p.serialized_module_map().clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
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

impl From<MoveModulePublish> for SuiMovePackage {
    fn from(m: MoveModulePublish) -> Self {
        Self {
            // In case of failed publish transaction, disassemble can fail, we can only return empty module map in that case.
            disassembled: disassemble_modules(m.modules.iter()).unwrap_or_default(),
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
            type_: format!("{}", info.type_),
            owner: info.owner,
            previous_transaction: info.previous_transaction,
        }
    }
}
