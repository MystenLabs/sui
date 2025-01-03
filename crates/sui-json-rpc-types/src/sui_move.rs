// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use colored::Colorize;
use itertools::Itertools;
use move_binary_format::file_format::{Ability, AbilitySet, DatatypeTyParameter, Visibility};
use move_binary_format::normalized::{
    Enum as NormalizedEnum, Field as NormalizedField, Function as SuiNormalizedFunction,
    Module as NormalizedModule, Struct as NormalizedStruct, Type as NormalizedType,
};
use move_core_types::annotated_value::{MoveStruct, MoveValue, MoveVariant};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::{Display, Formatter, Write};
use sui_macros::EnumVariantOrder;
use tracing::warn;

use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::SuiStructTag;

pub type SuiMoveTypeParameterIndex = u16;

#[cfg(test)]
#[path = "unit_tests/sui_move_tests.rs"]
mod sui_move_tests;

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum SuiMoveAbility {
    Copy,
    Drop,
    Store,
    Key,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct SuiMoveAbilitySet {
    pub abilities: Vec<SuiMoveAbility>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum SuiMoveVisibility {
    Private,
    Public,
    Friend,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiMoveStructTypeParameter {
    pub constraints: SuiMoveAbilitySet,
    pub is_phantom: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct SuiMoveNormalizedField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: SuiMoveNormalizedType,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiMoveNormalizedStruct {
    pub abilities: SuiMoveAbilitySet,
    pub type_parameters: Vec<SuiMoveStructTypeParameter>,
    pub fields: Vec<SuiMoveNormalizedField>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiMoveNormalizedEnum {
    pub abilities: SuiMoveAbilitySet,
    pub type_parameters: Vec<SuiMoveStructTypeParameter>,
    pub variants: BTreeMap<String, Vec<SuiMoveNormalizedField>>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum SuiMoveNormalizedType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    #[serde(rename_all = "camelCase")]
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiMoveNormalizedFunction {
    pub visibility: SuiMoveVisibility,
    pub is_entry: bool,
    pub type_parameters: Vec<SuiMoveAbilitySet>,
    pub parameters: Vec<SuiMoveNormalizedType>,
    pub return_: Vec<SuiMoveNormalizedType>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct SuiMoveModuleId {
    address: String,
    name: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiMoveNormalizedModule {
    pub file_format_version: u32,
    pub address: String,
    pub name: String,
    pub friends: Vec<SuiMoveModuleId>,
    pub structs: BTreeMap<String, SuiMoveNormalizedStruct>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub enums: BTreeMap<String, SuiMoveNormalizedEnum>,
    pub exposed_functions: BTreeMap<String, SuiMoveNormalizedFunction>,
}

impl PartialEq for SuiMoveNormalizedModule {
    fn eq(&self, other: &Self) -> bool {
        self.file_format_version == other.file_format_version
            && self.address == other.address
            && self.name == other.name
    }
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
            enums: module
                .enums
                .into_iter()
                .map(|(name, enum_)| (name.to_string(), SuiMoveNormalizedEnum::from(enum_)))
                .collect(),
            exposed_functions: module
                .functions
                .into_iter()
                .filter_map(|(name, function)| {
                    // TODO: Do we want to expose the private functions as well?
                    (function.is_entry || function.visibility != Visibility::Private)
                        .then(|| (name.to_string(), SuiMoveNormalizedFunction::from(function)))
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

impl From<NormalizedEnum> for SuiMoveNormalizedEnum {
    fn from(value: NormalizedEnum) -> Self {
        Self {
            abilities: value.abilities.into(),
            type_parameters: value
                .type_parameters
                .into_iter()
                .map(SuiMoveStructTypeParameter::from)
                .collect::<Vec<SuiMoveStructTypeParameter>>(),
            variants: value
                .variants
                .into_iter()
                .map(|variant| {
                    (
                        variant.name.to_string(),
                        variant
                            .fields
                            .into_iter()
                            .map(SuiMoveNormalizedField::from)
                            .collect::<Vec<SuiMoveNormalizedField>>(),
                    )
                })
                .collect::<BTreeMap<String, Vec<SuiMoveNormalizedField>>>(),
        }
    }
}

impl From<DatatypeTyParameter> for SuiMoveStructTypeParameter {
    fn from(type_parameter: DatatypeTyParameter) -> Self {
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
            NormalizedType::U16 => SuiMoveNormalizedType::U16,
            NormalizedType::U32 => SuiMoveNormalizedType::U32,
            NormalizedType::U64 => SuiMoveNormalizedType::U64,
            NormalizedType::U128 => SuiMoveNormalizedType::U128,
            NormalizedType::U256 => SuiMoveNormalizedType::U256,
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum ObjectValueKind {
    ByImmutableReference,
    ByMutableReference,
    ByValue,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub enum MoveFunctionArgType {
    Pure,
    Object(ObjectValueKind),
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq, EnumVariantOrder)]
#[serde(untagged, rename = "MoveValue")]
pub enum SuiMoveValue {
    // u64 and u128 are converted to String to avoid overflow
    Number(u32),
    Bool(bool),
    Address(SuiAddress),
    Vector(Vec<SuiMoveValue>),
    String(String),
    UID { id: ObjectID },
    Struct(SuiMoveStruct),
    Option(Box<Option<SuiMoveValue>>),
    Variant(SuiMoveVariant),
}

impl SuiMoveValue {
    /// Extract values from MoveValue without type information in json format
    pub fn to_json_value(self) -> Value {
        match self {
            SuiMoveValue::Struct(move_struct) => move_struct.to_json_value(),
            SuiMoveValue::Vector(values) => SuiMoveStruct::Runtime(values).to_json_value(),
            SuiMoveValue::Number(v) => json!(v),
            SuiMoveValue::Bool(v) => json!(v),
            SuiMoveValue::Address(v) => json!(v),
            SuiMoveValue::String(v) => json!(v),
            SuiMoveValue::UID { id } => json!({ "id": id }),
            SuiMoveValue::Option(v) => json!(v),
            SuiMoveValue::Variant(v) => v.to_json_value(),
        }
    }
}

impl Display for SuiMoveValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match self {
            SuiMoveValue::Number(value) => write!(writer, "{}", value)?,
            SuiMoveValue::Bool(value) => write!(writer, "{}", value)?,
            SuiMoveValue::Address(value) => write!(writer, "{}", value)?,
            SuiMoveValue::String(value) => write!(writer, "{}", value)?,
            SuiMoveValue::UID { id } => write!(writer, "{id}")?,
            SuiMoveValue::Struct(value) => write!(writer, "{}", value)?,
            SuiMoveValue::Option(value) => write!(writer, "{:?}", value)?,
            SuiMoveValue::Vector(vec) => {
                write!(
                    writer,
                    "{}",
                    vec.iter().map(|value| format!("{value}")).join(",\n")
                )?;
            }
            SuiMoveValue::Variant(value) => write!(writer, "{}", value)?,
        }
        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

impl From<MoveValue> for SuiMoveValue {
    fn from(value: MoveValue) -> Self {
        match value {
            MoveValue::U8(value) => SuiMoveValue::Number(value.into()),
            MoveValue::U16(value) => SuiMoveValue::Number(value.into()),
            MoveValue::U32(value) => SuiMoveValue::Number(value),
            MoveValue::U64(value) => SuiMoveValue::String(format!("{value}")),
            MoveValue::U128(value) => SuiMoveValue::String(format!("{value}")),
            MoveValue::U256(value) => SuiMoveValue::String(format!("{value}")),
            MoveValue::Bool(value) => SuiMoveValue::Bool(value),
            MoveValue::Vector(values) => {
                SuiMoveValue::Vector(values.into_iter().map(|value| value.into()).collect())
            }
            MoveValue::Struct(value) => {
                // Best effort Sui core type conversion
                let MoveStruct { type_, fields } = &value;
                if let Some(value) = try_convert_type(type_, fields) {
                    return value;
                }
                SuiMoveValue::Struct(value.into())
            }
            MoveValue::Signer(value) | MoveValue::Address(value) => {
                SuiMoveValue::Address(SuiAddress::from(ObjectID::from(value)))
            }
            MoveValue::Variant(MoveVariant {
                type_,
                variant_name,
                tag: _,
                fields,
            }) => SuiMoveValue::Variant(SuiMoveVariant {
                type_: type_.clone(),
                variant: variant_name.to_string(),
                fields: fields
                    .into_iter()
                    .map(|(id, value)| (id.into_string(), value.into()))
                    .collect::<BTreeMap<_, _>>(),
            }),
        }
    }
}

fn to_bytearray(value: &[MoveValue]) -> Option<Vec<u8>> {
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
        Some(bytearray)
    } else {
        None
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(rename = "MoveVariant")]
pub struct SuiMoveVariant {
    #[schemars(with = "String")]
    #[serde(rename = "type")]
    #[serde_as(as = "SuiStructTag")]
    pub type_: StructTag,
    pub variant: String,
    pub fields: BTreeMap<String, SuiMoveValue>,
}

impl SuiMoveVariant {
    pub fn to_json_value(self) -> Value {
        // We only care about values here, assuming type information is known at the client side.
        let fields = self
            .fields
            .into_iter()
            .map(|(key, value)| (key, value.to_json_value()))
            .collect::<BTreeMap<_, _>>();
        json!({
            "variant": self.variant,
            "fields": fields,
        })
    }
}

impl Display for SuiMoveVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        let SuiMoveVariant {
            type_,
            variant,
            fields,
        } = self;
        writeln!(writer)?;
        writeln!(writer, "  {}: {type_}", "type".bold().bright_black())?;
        writeln!(writer, "  {}: {variant}", "variant".bold().bright_black())?;
        for (name, value) in fields {
            let value = format!("{}", value);
            let value = if value.starts_with('\n') {
                indent(&value, 2)
            } else {
                value
            };
            writeln!(writer, "  {}: {value}", name.bold().bright_black())?;
        }

        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Eq, PartialEq, EnumVariantOrder)]
#[serde(untagged, rename = "MoveStruct")]
pub enum SuiMoveStruct {
    Runtime(Vec<SuiMoveValue>),
    WithTypes {
        #[schemars(with = "String")]
        #[serde(rename = "type")]
        #[serde_as(as = "SuiStructTag")]
        type_: StructTag,
        fields: BTreeMap<String, SuiMoveValue>,
    },
    WithFields(BTreeMap<String, SuiMoveValue>),
}

impl SuiMoveStruct {
    /// Extract values from MoveStruct without type information in json format
    pub fn to_json_value(self) -> Value {
        // Unwrap MoveStructs
        match self {
            SuiMoveStruct::Runtime(values) => {
                let values = values
                    .into_iter()
                    .map(|value| value.to_json_value())
                    .collect::<Vec<_>>();
                json!(values)
            }
            // We only care about values here, assuming struct type information is known at the client side.
            SuiMoveStruct::WithTypes { type_: _, fields } | SuiMoveStruct::WithFields(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|(key, value)| (key, value.to_json_value()))
                    .collect::<BTreeMap<_, _>>();
                json!(fields)
            }
        }
    }

    pub fn field_value(&self, field_name: &str) -> Option<SuiMoveValue> {
        match self {
            SuiMoveStruct::WithFields(fields) => fields.get(field_name).cloned(),
            SuiMoveStruct::WithTypes { type_: _, fields } => fields.get(field_name).cloned(),
            _ => None,
        }
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

fn indent<T: Display>(d: &T, indent: usize) -> String {
    d.to_string()
        .lines()
        .map(|line| format!("{:indent$}{}", "", line))
        .join("\n")
}

fn try_convert_type(type_: &StructTag, fields: &[(Identifier, MoveValue)]) -> Option<SuiMoveValue> {
    let struct_name = format!(
        "0x{}::{}::{}",
        type_.address.short_str_lossless(),
        type_.module,
        type_.name
    );
    let mut values = fields
        .iter()
        .map(|(id, value)| (id.to_string(), value))
        .collect::<BTreeMap<_, _>>();
    match struct_name.as_str() {
        "0x1::string::String" | "0x1::ascii::String" => {
            if let Some(MoveValue::Vector(bytes)) = values.remove("bytes") {
                return to_bytearray(bytes)
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(SuiMoveValue::String);
            }
        }
        "0x2::url::Url" => {
            return values.remove("url").cloned().map(SuiMoveValue::from);
        }
        "0x2::object::ID" => {
            return values.remove("bytes").cloned().map(SuiMoveValue::from);
        }
        "0x2::object::UID" => {
            let id = values.remove("id").cloned().map(SuiMoveValue::from);
            if let Some(SuiMoveValue::Address(address)) = id {
                return Some(SuiMoveValue::UID {
                    id: ObjectID::from(address),
                });
            }
        }
        "0x2::balance::Balance" => {
            return values.remove("value").cloned().map(SuiMoveValue::from);
        }
        "0x1::option::Option" => {
            if let Some(MoveValue::Vector(values)) = values.remove("vec") {
                return Some(SuiMoveValue::Option(Box::new(
                    // in Move option is modeled as vec of 1 element
                    values.first().cloned().map(SuiMoveValue::from),
                )));
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
        SuiMoveStruct::WithTypes {
            type_: move_struct.type_,
            fields: move_struct
                .fields
                .into_iter()
                .map(|(id, value)| (id.into_string(), value.into()))
                .collect(),
        }
    }
}
