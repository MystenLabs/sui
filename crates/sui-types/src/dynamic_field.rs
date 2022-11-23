// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::{MoveStruct, MoveValue};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::base_types::ObjectDigest;
use crate::error::{SuiError, SuiResult};
use crate::id::ID;
use crate::{ObjectID, SequenceNumber, SUI_FRAMEWORK_ADDRESS};

#[derive(Clone, Serialize, Deserialize, JsonSchema, Ord, PartialOrd, Eq, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldInfo {
    pub name: String,
    pub type_: DynamicFieldType,
    pub object_type: String,
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum DynamicFieldType {
    #[serde(rename_all = "camelCase")]
    DynamicField {
        wrapped_object_id: ObjectID,
    },
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
    ) -> SuiResult<(String, DynamicFieldType, ObjectID)> {
        let name = extract_field_from_move_struct(&move_struct, "name").ok_or_else(|| {
            SuiError::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        let value = extract_field_from_move_struct(&move_struct, "value").ok_or_else(|| {
            SuiError::ObjectDeserializationError {
                error: "Cannot extract [value] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        // Extract value from value option
        let value = match &value {
            MoveValue::Struct(MoveStruct::WithTypes { type_: _, fields }) => match fields.first() {
                Some((_, MoveValue::Vector(v))) => v.first().cloned(),
                _ => None,
            },
            _ => None,
        }
        .ok_or_else(|| SuiError::ObjectDeserializationError {
            error: "Cannot extract optional value".to_string(),
        })?;

        let object_id =
            extract_object_id(&value).ok_or_else(|| SuiError::ObjectDeserializationError {
                error: format!(
                    "Cannot extract dynamic object's object id from Field::value, {:?}",
                    value
                ),
            })?;

        Ok(if is_dynamic_object(move_struct) {
            let name = match name {
                MoveValue::Struct(s) => extract_field_from_move_struct(&s, "name"),
                _ => None,
            }
            .ok_or_else(|| SuiError::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_object_field::Wrapper."
                    .to_string(),
            })?;

            (name.to_string(), DynamicFieldType::DynamicObject, object_id)
        } else {
            (
                name.to_string(),
                DynamicFieldType::DynamicField {
                    wrapped_object_id: object_id,
                },
                object_id,
            )
        })
    }
}

fn extract_field_from_move_struct(move_struct: &MoveStruct, field_name: &str) -> Option<MoveValue> {
    match move_struct {
        MoveStruct::WithTypes { fields, .. } => fields.iter().find_map(|(id, value)| {
            if id.to_string() == field_name {
                Some(value.clone())
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn extract_object_id(value: &MoveValue) -> Option<ObjectID> {
    match value {
        MoveValue::Struct(MoveStruct::WithTypes { type_, fields }) => {
            if type_ == &ID::type_() {
                match fields.first() {
                    Some((_, MoveValue::Address(addr))) => Some(ObjectID::from(*addr)),
                    _ => None,
                }
            } else {
                for (_, value) in fields {
                    let id = extract_object_id(value);
                    if id.is_some() {
                        return id;
                    }
                }
                None
            }
        }
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
