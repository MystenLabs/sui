// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectDigest;
use crate::error::{SuiError, SuiResult};
use crate::{ObjectID, SequenceNumber, SUI_FRAMEWORK_ADDRESS};
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::{MoveStruct, MoveValue};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use std::fmt::{Display, Formatter};

#[derive(Clone, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldInfo {
    pub name: DynamicFieldName,
    pub type_: DynamicFieldType,
    pub object_type: String,
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldName {
    pub type_: String,
    // Bincode does not like serde_json::Value, rocksdb will not insert the value without this hack.
    #[schemars(with = "Value")]
    #[serde_as(as = "DisplayFromStr")]
    pub value: Value,
}

impl Display for DynamicFieldName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.type_, self.value)
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum DynamicFieldType {
    #[serde(rename_all = "camelCase")]
    DynamicField,
    DynamicObject,
}

impl DynamicFieldInfo {
    pub fn is_dynamic_field(tag: &StructTag) -> bool {
        tag.address == SUI_FRAMEWORK_ADDRESS
            && tag.module.as_str() == "dynamic_field"
            && tag.name.as_str() == "Field"
    }

    pub fn parse_move_object(
        move_struct: &MoveStruct,
    ) -> SuiResult<(MoveValue, DynamicFieldType, ObjectID)> {
        let name = extract_field_from_move_struct(move_struct, "name").ok_or_else(|| {
            SuiError::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        let value = extract_field_from_move_struct(move_struct, "value").ok_or_else(|| {
            SuiError::ObjectDeserializationError {
                error: "Cannot extract [value] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        Ok(if is_dynamic_object(move_struct) {
            let name = match name {
                MoveValue::Struct(name_struct) => {
                    extract_field_from_move_struct(name_struct, "name")
                }
                _ => None,
            }
            .ok_or_else(|| SuiError::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_object_field::Wrapper."
                    .to_string(),
            })?;
            // ID extracted from the wrapper object
            let object_id =
                extract_id_value(value).ok_or_else(|| SuiError::ObjectDeserializationError {
                    error: format!(
                        "Cannot extract dynamic object's object id from \
                        sui::dynamic_field::Field, {value:?}"
                    ),
                })?;
            (name.clone(), DynamicFieldType::DynamicObject, object_id)
        } else {
            // ID of the Field object
            let object_id = extract_object_id(move_struct).ok_or_else(|| {
                SuiError::ObjectDeserializationError {
                    error: format!(
                        "Cannot extract dynamic object's object id from \
                        sui::dynamic_field::Field, {move_struct:?}",
                    ),
                }
            })?;
            (name.clone(), DynamicFieldType::DynamicField, object_id)
        })
    }
}

fn extract_field_from_move_struct<'a>(
    move_struct: &'a MoveStruct,
    field_name: &str,
) -> Option<&'a MoveValue> {
    match move_struct {
        MoveStruct::WithTypes { fields, .. } | MoveStruct::WithFields(fields) => {
            fields.iter().find_map(|(id, value)| {
                if id.to_string() == field_name {
                    Some(value)
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

fn extract_object_id(value: &MoveStruct) -> Option<ObjectID> {
    // id:UID is the first value in an object
    let uid_value = match value {
        MoveStruct::Runtime(fields) => fields.get(0)?,
        MoveStruct::WithFields(fields) | MoveStruct::WithTypes { fields, .. } => &fields.get(0)?.1,
    };
    // id is the first value in UID
    let id_value = match uid_value {
        MoveValue::Struct(MoveStruct::Runtime(fields)) => fields.get(0)?,
        MoveValue::Struct(
            MoveStruct::WithFields(fields) | MoveStruct::WithTypes { fields, .. },
        ) => &fields.get(0)?.1,
        _ => return None,
    };
    extract_id_value(id_value)
}

fn extract_id_value(id_value: &MoveValue) -> Option<ObjectID> {
    // the id struct has a single bytes field
    let id_bytes_value = match id_value {
        MoveValue::Struct(MoveStruct::Runtime(fields)) => fields.get(0)?,
        MoveValue::Struct(
            MoveStruct::WithFields(fields) | MoveStruct::WithTypes { fields, .. },
        ) => &fields.get(0)?.1,
        _ => return None,
    };
    // the bytes field should be an address
    match id_bytes_value {
        MoveValue::Address(addr) => Some(ObjectID::from(*addr)),
        _ => None,
    }
}

pub fn is_dynamic_object(move_struct: &MoveStruct) -> bool {
    match move_struct {
        MoveStruct::WithTypes { type_, .. } => {
            matches!(&type_.type_params[0], TypeTag::Struct(tag) if tag.address == SUI_FRAMEWORK_ADDRESS
        && tag.module.as_str() == "dynamic_object_field"
        && tag.name.as_str() == "Wrapper")
        }
        _ => false,
    }
}
