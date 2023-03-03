// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use colored::Colorize;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectRef, ObjectType, SequenceNumber, TransactionDigest,
};
use sui_types::error::{UserInputError, UserInputResult};
use sui_types::gas_coin::GasCoin;
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, MoveObject, Object, ObjectRead, Owner};
use sui_types::parse_sui_struct_tag;

use crate::{
    SuiData, SuiMoveObject, SuiMoveStruct, SuiMoveValue, SuiObject, SuiParsedData, SuiRawData,
};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "status", content = "details", rename = "ObjectRead")]
pub enum SuiObjectWithStatus {
    Exists(SuiObjectData),
    NotExists(ObjectID),
    Deleted(ObjectRef),
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ObjectData")]
pub struct SuiObjectData {
    pub object_id: ObjectID,
    /// Object version.
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: ObjectDigest,
    /// The type of the object. Default to be Some(T) unless SuiObjectContentOptions.showType is set to False
    #[serde(rename = "type")]
    pub type_: Option<String>,
    // Default to be None because otherwise it will be repeated for the getObjectsOwnedByAddress endpoint
    /// The owner of this object. Default to be None unless SuiObjectContentOptions.showOwner is set
    pub owner: Option<Owner>,
    /// The digest of the transaction that created or last mutated this object. Default to be None unless
    /// SuiObjectContentOptions.showPreviousTransaction is set
    pub previous_transaction: Option<TransactionDigest>,
    /// The amount of SUI we would rebate if this object gets deleted.
    /// This number is re-calculated each time the object is mutated based on
    /// the present storage gas price.
    pub storage_rebate: Option<u64>,
    // TODO: uncomment the following in the next PR
    // /// The Display metadata for frontend UI rendering
    // pub display: Option<BTreeMap<String, String>>,
    /// Move object content or package content, default to be None unless SuiObjectContentOptions.showContent is set
    pub content: Option<SuiParsedData>,
    /// Move object content or package content in BCS, default to be None unless SuiObjectContentOptions.showContent is set
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ObjectContentOptions")]
pub struct SuiObjectContentOptions {
    /// Whether to show the type of the object. Default to be True
    pub show_type: Option<bool>,
    /// Whether to show the owner of the object. Default to be False
    pub show_owner: Option<bool>,
    /// Whether to show the previous transaction digest of the object. Default to be False
    pub show_previous_transaction: Option<bool>,
    // uncomment the following in the next PR
    // /// Whether to show the Display metadata of the object for frontend rendering. Default to be False
    // pub show_display: Option<bool>,
    /// Whether to show the content(i.e., package content or Move struct content) of the object.
    /// Default to be False
    pub show_content: Option<bool>,
    /// Whether to show the content in BCS format. Default to be False
    pub show_bcs: Option<bool>,
    /// Whether to show the storage rebate of the object. Default to be False
    pub show_storage_rebate: Option<bool>,
}

impl SuiObjectContentOptions {
    pub fn bcs_only() -> Self {
        Self {
            show_bcs: Some(true),
            show_type: Some(false),
            show_owner: Some(false),
            show_previous_transaction: Some(false),
            // uncomment the following in the next PR
            // show_display: Some(false),
            show_content: Some(false),
            show_storage_rebate: Some(false),
        }
    }

    /// return BCS data and all other metadata such as storage rebate
    pub fn bcs_lossless() -> Self {
        Self {
            show_bcs: Some(true),
            // Skip because this is inside the SuiRawData
            show_type: Some(false),
            show_owner: Some(true),
            show_previous_transaction: Some(true),
            // uncomment the following in the next PR
            // show_display: Some(false),
            show_content: Some(false),
            show_storage_rebate: Some(true),
        }
    }

    /// return full content except bcs
    pub fn full_content() -> Self {
        Self {
            show_bcs: Some(false),
            // Skip because this is inside the SuiRawData
            show_type: Some(true),
            show_owner: Some(true),
            show_previous_transaction: Some(true),
            // uncomment the following in the next PR
            // show_display: Some(false),
            show_content: Some(true),
            show_storage_rebate: Some(true),
        }
    }
}

impl Default for SuiObjectContentOptions {
    fn default() -> Self {
        Self {
            show_type: Some(true),
            show_owner: Some(false),
            show_previous_transaction: Some(false),
            // uncomment the following in the next PR
            // show_display: Some(false),
            show_content: Some(false),
            show_bcs: Some(false),
            show_storage_rebate: Some(false),
        }
    }
}

impl TryFrom<(ObjectRead, Option<SuiObjectContentOptions>)> for SuiObjectWithStatus {
    type Error = anyhow::Error;

    fn try_from(
        (object_read, options): (ObjectRead, Option<SuiObjectContentOptions>),
    ) -> Result<Self, Self::Error> {
        let options = options.unwrap_or_default();
        let default_options = SuiObjectContentOptions::default();
        // It is safe to unwrap because default value are all Some(bool)
        let show_type = options
            .show_type
            .unwrap_or_else(|| default_options.show_type.unwrap());
        let show_owner = options
            .show_owner
            .unwrap_or_else(|| default_options.show_owner.unwrap());
        let show_previous_transaction = options
            .show_previous_transaction
            .unwrap_or_else(|| default_options.show_previous_transaction.unwrap());
        let show_content = options
            .show_content
            .unwrap_or_else(|| default_options.show_content.unwrap());
        let show_bcs = options
            .show_bcs
            .unwrap_or_else(|| default_options.show_bcs.unwrap());
        let show_storage_rebate = options
            .show_storage_rebate
            .unwrap_or_else(|| default_options.show_storage_rebate.unwrap());

        match object_read {
            ObjectRead::NotExists(id) => Ok(Self::NotExists(id)),
            ObjectRead::Exists(object_ref, o, layout) => {
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

                Ok(Self::Exists(SuiObjectData {
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
                }))
            }
            ObjectRead::Deleted(oref) => Ok(Self::Deleted(oref)),
        }
    }
}

impl SuiObjectWithStatus {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> UserInputResult<&SuiObjectData> {
        match &self {
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted { object_ref: *oref }),
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
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted { object_ref: oref }),
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

impl TryInto<Object> for SuiObject<SuiRawData> {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Object, Self::Error> {
        let protocol_config = ProtocolConfig::get_for_min_version();
        let data = match self.data {
            SuiRawData::MoveObject(o) => {
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
            SuiRawData::Package(p) => Data::Package(MovePackage::new(
                p.id,
                self.reference.version,
                &p.module_map,
                protocol_config.max_move_package_size(),
            )?),
        };
        Ok(Object {
            data,
            owner: self.owner,
            previous_transaction: self.previous_transaction,
            storage_rebate: self.storage_rebate,
        })
    }
}
