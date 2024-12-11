// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;

use anyhow::{anyhow, bail};
use fastcrypto::encoding::{Encoding, Hex};
use move_binary_format::CompiledModule;
use move_binary_format::{binary_config::BinaryConfig, file_format::SignatureToken};
use move_bytecode_utils::resolve_struct;
pub use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::annotated_value::{MoveFieldLayout, MoveVariant};
use move_core_types::u256::U256;
use move_core_types::{
    annotated_value::{MoveStruct, MoveValue},
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    runtime_value as R,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value as JsonValue};

use sui_types::base_types::{
    is_primitive_type_tag, move_ascii_str_layout, move_utf8_str_layout, ObjectID, SuiAddress,
    TxContext, TxContextKind, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_UTF8_STR,
    STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME, STD_OPTION_MODULE_NAME, STD_OPTION_STRUCT_NAME,
    STD_UTF8_MODULE_NAME, STD_UTF8_STRUCT_NAME,
};
use sui_types::id::{self, ID, RESOLVED_SUI_ID};
use sui_types::move_package::MovePackage;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::transfer::RESOLVED_RECEIVING_STRUCT;
use sui_types::MOVE_STDLIB_ADDRESS;

const HEX_PREFIX: &str = "0x";

#[cfg(test)]
mod tests;

/// A list of error categories encountered when parsing numbers.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum SuiJsonValueErrorKind {
    /// JSON value must be of specific types.
    ValueTypeNotAllowed,

    /// JSON arrays must be homogeneous.
    ArrayNotHomogeneous,
}

#[derive(Debug)]
pub struct SuiJsonValueError {
    kind: SuiJsonValueErrorKind,
    val: JsonValue,
}

impl SuiJsonValueError {
    pub fn new(val: &JsonValue, kind: SuiJsonValueErrorKind) -> Self {
        Self {
            kind,
            val: val.clone(),
        }
    }
}

impl std::error::Error for SuiJsonValueError {}

impl fmt::Display for SuiJsonValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let err_str = match self.kind {
            SuiJsonValueErrorKind::ValueTypeNotAllowed => {
                format!("JSON value type {} not allowed.", self.val)
            }
            SuiJsonValueErrorKind::ArrayNotHomogeneous => {
                format!("Array not homogeneous. Mismatched value: {}.", self.val)
            }
        };
        write!(f, "{err_str}")
    }
}

// Intermediate type to hold resolved args
#[derive(Eq, PartialEq, Debug)]
pub enum ResolvedCallArg {
    Object(ObjectID),
    Pure(Vec<u8>),
    ObjVec(Vec<ObjectID>),
}

#[derive(Eq, PartialEq, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SuiJsonValue(JsonValue);
impl SuiJsonValue {
    pub fn new(json_value: JsonValue) -> Result<SuiJsonValue, anyhow::Error> {
        Self::check_value(&json_value)?;
        Ok(Self(json_value))
    }

    fn check_value(json_value: &JsonValue) -> Result<(), anyhow::Error> {
        match json_value {
            // No checks needed for Bool and String
            JsonValue::Bool(_) | JsonValue::String(_) => (),
            JsonValue::Number(n) => {
                // Must be castable to u64
                if !n.is_u64() {
                    return Err(anyhow!(
                        "{n} not allowed. Number must be unsigned integer of at most u32"
                    ));
                }
            }
            // Must be homogeneous
            JsonValue::Array(a) => {
                // Fail if not homogeneous
                check_valid_homogeneous(&JsonValue::Array(a.to_vec()))?
            }
            JsonValue::Object(v) => {
                for (_, value) in v {
                    Self::check_value(value)?;
                }
            }
            JsonValue::Null => bail!("Null not allowed."),
        };
        Ok(())
    }

    pub fn from_object_id(id: ObjectID) -> SuiJsonValue {
        Self(JsonValue::String(id.to_hex_uncompressed()))
    }

    pub fn to_bcs_bytes(&self, ty: &MoveTypeLayout) -> Result<Vec<u8>, anyhow::Error> {
        let move_value = Self::to_move_value(&self.0, ty)?;
        R::MoveValue::simple_serialize(&move_value)
            .ok_or_else(|| anyhow!("Unable to serialize {:?}. Expected {}", move_value, ty))
    }

    pub fn from_bcs_bytes(
        layout: Option<&MoveTypeLayout>,
        bytes: &[u8],
    ) -> Result<Self, anyhow::Error> {
        let json = if let Some(layout) = layout {
            // Try to convert Vec<u8> inputs into string
            fn try_parse_string(layout: &MoveTypeLayout, bytes: &[u8]) -> Option<String> {
                if let MoveTypeLayout::Vector(t) = layout {
                    if let MoveTypeLayout::U8 = **t {
                        return bcs::from_bytes::<String>(bytes).ok();
                    }
                }
                None
            }
            if let Some(s) = try_parse_string(layout, bytes) {
                json!(s)
            } else {
                let result = BoundedVisitor::deserialize_value(bytes, layout).map_or_else(
                    |_| {
                        // fallback to array[u8] if fail to convert to json.
                        JsonValue::Array(
                            bytes
                                .iter()
                                .map(|b| JsonValue::Number(Number::from(*b)))
                                .collect(),
                        )
                    },
                    |move_value| {
                        move_value_to_json(&move_value).unwrap_or_else(|| {
                            // fallback to array[u8] if fail to convert to json.
                            JsonValue::Array(
                                bytes
                                    .iter()
                                    .map(|b| JsonValue::Number(Number::from(*b)))
                                    .collect(),
                            )
                        })
                    },
                );
                result
            }
        } else {
            json!(bytes)
        };
        SuiJsonValue::new(json)
    }

    pub fn to_json_value(&self) -> JsonValue {
        self.0.clone()
    }

    pub fn to_sui_address(&self) -> anyhow::Result<SuiAddress> {
        json_value_to_sui_address(&self.0)
    }

    fn handle_inner_struct_layout(
        inner_vec: &[MoveFieldLayout],
        val: &JsonValue,
        ty: &MoveTypeLayout,
        s: &String,
    ) -> Result<R::MoveValue, anyhow::Error> {
        // delegate MoveValue construction to the case when JsonValue::String and
        // MoveTypeLayout::Vector are handled to get an address (with 0x string
        // prefix) or a vector of u8s (no prefix)
        debug_assert!(matches!(val, JsonValue::String(_)));

        if inner_vec.len() != 1 {
            bail!(
                "Cannot convert string arg {s} to {ty} which is expected \
                 to be a struct with one field"
            );
        }

        match &inner_vec[0].layout {
            MoveTypeLayout::Vector(inner) => match **inner {
                MoveTypeLayout::U8 => Ok(R::MoveValue::Struct(R::MoveStruct(vec![
                    Self::to_move_value(val, &inner_vec[0].layout.clone())?,
                ]))),
                MoveTypeLayout::Address => Ok(R::MoveValue::Struct(R::MoveStruct(vec![
                    Self::to_move_value(val, &MoveTypeLayout::Address)?,
                ]))),
                _ => bail!(
                    "Cannot convert string arg {s} to {ty} \
                             which is expected to be a struct \
                             with one field of address or u8 vector type"
                ),
            },
            MoveTypeLayout::Struct(struct_layout) if struct_layout.type_ == ID::type_() => {
                Ok(R::MoveValue::Struct(R::MoveStruct(vec![
                    Self::to_move_value(val, &inner_vec[0].layout.clone())?,
                ])))
            }
            _ => bail!(
                "Cannot convert string arg {s} to {ty} which is expected \
                 to be a struct with one field of a vector type"
            ),
        }
    }

    pub fn to_move_value(
        val: &JsonValue,
        ty: &MoveTypeLayout,
    ) -> Result<R::MoveValue, anyhow::Error> {
        Ok(match (val, ty) {
            // Bool to Bool is simple
            (JsonValue::Bool(b), MoveTypeLayout::Bool) => R::MoveValue::Bool(*b),

            // In constructor, we have already checked that the JSON number is unsigned int of at most U32
            (JsonValue::Number(n), MoveTypeLayout::U8) => match n.as_u64() {
                Some(x) => R::MoveValue::U8(u8::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u8 allowed.", n)),
            },
            (JsonValue::Number(n), MoveTypeLayout::U16) => match n.as_u64() {
                Some(x) => R::MoveValue::U16(u16::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u16 allowed.", n)),
            },
            (JsonValue::Number(n), MoveTypeLayout::U32) => match n.as_u64() {
                Some(x) => R::MoveValue::U32(u32::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u32 allowed.", n)),
            },

            // u8, u16, u32, u64, u128, u256 can be encoded as String
            (JsonValue::String(s), MoveTypeLayout::U8) => {
                R::MoveValue::U8(u8::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U16) => {
                R::MoveValue::U16(u16::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U32) => {
                R::MoveValue::U32(u32::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U64) => {
                R::MoveValue::U64(u64::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U128) => {
                R::MoveValue::U128(u128::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U256) => {
                R::MoveValue::U256(convert_string_to_u256(s.as_str())?)
            }
            // For ascii and utf8 strings
            (JsonValue::String(s), MoveTypeLayout::Struct(struct_layout))
                if is_move_string_type(&struct_layout.type_) =>
            {
                R::MoveValue::Vector(s.as_bytes().iter().copied().map(R::MoveValue::U8).collect())
            }
            // For ID
            (JsonValue::String(s), MoveTypeLayout::Struct(struct_layout))
                if struct_layout.type_ == ID::type_() =>
            {
                if struct_layout.fields.len() != 1 {
                    bail!(
                        "Cannot convert string arg {s} to {} which is expected to be a struct with one field", struct_layout.type_
                    );
                };
                let addr = SuiAddress::from_str(s)?;
                R::MoveValue::Address(addr.into())
            }
            (JsonValue::Object(o), MoveTypeLayout::Struct(struct_layout)) => {
                let mut field_values = vec![];
                for layout in struct_layout.fields.iter() {
                    let field = o
                        .get(layout.name.as_str())
                        .ok_or_else(|| anyhow!("Missing field {} for struct {ty}", layout.name))?;
                    field_values.push(Self::to_move_value(field, &layout.layout)?);
                }
                R::MoveValue::Struct(R::MoveStruct(field_values))
            }
            // Unnest fields
            (value, MoveTypeLayout::Struct(struct_layout)) if struct_layout.fields.len() == 1 => {
                Self::to_move_value(value, &struct_layout.fields[0].layout)?
            }
            (JsonValue::String(s), MoveTypeLayout::Vector(t)) => {
                match &**t {
                    MoveTypeLayout::U8 => {
                        // We can encode U8 Vector as string in 2 ways
                        // 1. If it starts with 0x, we treat it as hex strings, where each pair is a
                        //    byte
                        // 2. If it does not start with 0x, we treat each character as an ASCII
                        //    encoded byte
                        // We have to support both for the convenience of the user. This is because
                        // sometime we need Strings as arg Other times we need vec of hex bytes for
                        // address. Issue is both Address and Strings are represented as Vec<u8> in
                        // Move call
                        let vec = if s.starts_with(HEX_PREFIX) {
                            // If starts with 0x, treat as hex vector
                            Hex::decode(s).map_err(|e| anyhow!(e))?
                        } else {
                            // Else raw bytes
                            s.as_bytes().to_vec()
                        };
                        R::MoveValue::Vector(vec.iter().copied().map(R::MoveValue::U8).collect())
                    }
                    MoveTypeLayout::Struct(struct_layout) => {
                        Self::handle_inner_struct_layout(&struct_layout.fields, val, ty, s)?
                    }
                    _ => bail!("Cannot convert string arg {s} to {ty}"),
                }
            }

            // We have already checked that the array is homogeneous in the constructor
            (JsonValue::Array(a), MoveTypeLayout::Vector(inner)) => {
                // Recursively build an IntermediateValue array
                R::MoveValue::Vector(
                    a.iter()
                        .map(|i| Self::to_move_value(i, inner))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }

            (v, MoveTypeLayout::Address) => {
                let addr = json_value_to_sui_address(v)?;
                R::MoveValue::Address(addr.into())
            }

            _ => bail!("Unexpected arg {val:?} for expected type {ty:?}"),
        })
    }
}

impl Debug for SuiJsonValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn json_value_to_sui_address(value: &JsonValue) -> anyhow::Result<SuiAddress> {
    match value {
        JsonValue::String(s) => {
            let s = s.trim().to_lowercase();
            if !s.starts_with(HEX_PREFIX) {
                bail!("Address hex string must start with 0x.",);
            }
            Ok(SuiAddress::from_str(&s)?)
        }
        JsonValue::Array(bytes) => {
            fn value_to_byte_array(v: &Vec<JsonValue>) -> Option<Vec<u8>> {
                let mut bytes = vec![];
                for b in v {
                    let b = b.as_u64()?;
                    if b <= u8::MAX as u64 {
                        bytes.push(b as u8);
                    } else {
                        return None;
                    }
                }
                Some(bytes)
            }
            let bytes = value_to_byte_array(bytes)
                .ok_or_else(|| anyhow!("Invalid input: Cannot parse input into SuiAddress."))?;
            Ok(SuiAddress::try_from(bytes)?)
        }
        v => bail!("Unexpected arg {v} for expected type address"),
    }
}

fn move_value_to_json(move_value: &MoveValue) -> Option<JsonValue> {
    Some(match move_value {
        MoveValue::Vector(values) => JsonValue::Array(
            values
                .iter()
                .map(move_value_to_json)
                .collect::<Option<_>>()?,
        ),
        MoveValue::Bool(v) => json!(v),
        MoveValue::Signer(v) | MoveValue::Address(v) => json!(SuiAddress::from(*v).to_string()),
        MoveValue::U8(v) => json!(v),
        MoveValue::U64(v) => json!(v.to_string()),
        MoveValue::U128(v) => json!(v.to_string()),
        MoveValue::U16(v) => json!(v),
        MoveValue::U32(v) => json!(v),
        MoveValue::U256(v) => json!(v.to_string()),
        MoveValue::Struct(move_struct) => match move_struct {
            MoveStruct { fields, type_ } if is_move_string_type(type_) => {
                // ascii::string and utf8::string has a single bytes field.
                let (_, v) = fields.first()?;
                let string: String = bcs::from_bytes(&v.simple_serialize()?).ok()?;
                json!(string)
            }
            MoveStruct { fields, type_ } if is_move_option_type(type_) => {
                // option has a single vec field.
                let (_, v) = fields.first()?;
                if let MoveValue::Vector(v) = v {
                    JsonValue::Array(v.iter().filter_map(move_value_to_json).collect::<Vec<_>>())
                } else {
                    return None;
                }
            }
            MoveStruct { fields, type_ } if type_ == &ID::type_() => {
                // option has a single vec field.
                let (_, v) = fields.first()?;
                if let MoveValue::Address(address) = v {
                    json!(SuiAddress::from(*address))
                } else {
                    return None;
                }
            }
            // We only care about values here, assuming struct type information is known at the client side.
            MoveStruct { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|(key, value)| (key, move_value_to_json(value)))
                    .collect::<BTreeMap<_, _>>();
                json!(fields)
            }
        },
        // Don't return the type assuming type information is known at the client side.
        MoveValue::Variant(MoveVariant {
            type_: _,
            tag: _,
            variant_name,
            fields,
        }) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key, move_value_to_json(value)))
                .collect::<BTreeMap<_, _>>();
            json!({
                "variant": variant_name.to_string(),
                "fields": fields,
            })
        }
    })
}

fn is_move_string_type(tag: &StructTag) -> bool {
    (tag.address == MOVE_STDLIB_ADDRESS
        && tag.module.as_ident_str() == STD_UTF8_MODULE_NAME
        && tag.name.as_ident_str() == STD_UTF8_STRUCT_NAME)
        || (tag.address == MOVE_STDLIB_ADDRESS
            && tag.module.as_ident_str() == STD_ASCII_MODULE_NAME
            && tag.name.as_ident_str() == STD_ASCII_STRUCT_NAME)
}
fn is_move_option_type(tag: &StructTag) -> bool {
    tag.address == MOVE_STDLIB_ADDRESS
        && tag.module.as_ident_str() == STD_OPTION_MODULE_NAME
        && tag.name.as_ident_str() == STD_OPTION_STRUCT_NAME
}

impl FromStr for SuiJsonValue {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        fn try_escape_array(s: &str) -> JsonValue {
            let s = s.trim();
            if s.starts_with('[') && s.ends_with(']') {
                if let Some(s) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    return JsonValue::Array(s.split(',').map(try_escape_array).collect());
                }
            }
            json!(s)
        }
        // if serde_json fails, the failure usually cause by missing quote escapes, try parse array manually.
        SuiJsonValue::new(serde_json::from_str(s).unwrap_or_else(|_| try_escape_array(s)))
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Hash)]
enum ValidJsonType {
    Bool,
    Number,
    String,
    Array,
    // Matches any type
    Any,
}

/// Check via BFS
/// The invariant is that all types at a given level must be the same or be empty, and all must be valid
pub fn check_valid_homogeneous(val: &JsonValue) -> Result<(), SuiJsonValueError> {
    let mut deq: VecDeque<&JsonValue> = VecDeque::new();
    deq.push_back(val);
    check_valid_homogeneous_rec(&mut deq)
}

/// Check via BFS
/// The invariant is that all types at a given level must be the same or be empty
fn check_valid_homogeneous_rec(curr_q: &mut VecDeque<&JsonValue>) -> Result<(), SuiJsonValueError> {
    if curr_q.is_empty() {
        // Nothing to do
        return Ok(());
    }
    // Queue for the next level
    let mut next_q = VecDeque::new();
    // The types at this level must be the same
    let mut level_type = ValidJsonType::Any;

    // Process all in this queue/level
    while let Some(v) = curr_q.pop_front() {
        let curr = match v {
            JsonValue::Bool(_) => ValidJsonType::Bool,
            JsonValue::Number(x) if x.is_u64() => ValidJsonType::Number,
            JsonValue::String(_) => ValidJsonType::String,
            JsonValue::Array(w) => {
                // Add to the next level
                w.iter().for_each(|t| next_q.push_back(t));
                ValidJsonType::Array
            }
            // Not valid
            _ => {
                return Err(SuiJsonValueError::new(
                    v,
                    SuiJsonValueErrorKind::ValueTypeNotAllowed,
                ))
            }
        };

        if level_type == ValidJsonType::Any {
            // Update the level with the first found type
            level_type = curr;
        } else if level_type != curr {
            // Mismatch in the level
            return Err(SuiJsonValueError::new(
                v,
                SuiJsonValueErrorKind::ArrayNotHomogeneous,
            ));
        }
    }
    // Process the next level
    check_valid_homogeneous_rec(&mut next_q)
}

/// Checks if a give SignatureToken represents a primitive type and, if so, returns MoveTypeLayout
/// for this type (if available). The reason we need to return both information about whether a
/// SignatureToken represents a primitive and an Option representing MoveTypeLayout is that there
/// can be signature tokens that represent primitives but that do not have corresponding
/// MoveTypeLayout (e.g., SignatureToken::DatatypeInstantiation).
pub fn primitive_type(
    view: &CompiledModule,
    type_args: &[TypeTag],
    param: &SignatureToken,
) -> Option<MoveTypeLayout> {
    Some(match param {
        SignatureToken::Bool => MoveTypeLayout::Bool,
        SignatureToken::U8 => MoveTypeLayout::U8,
        SignatureToken::U16 => MoveTypeLayout::U16,
        SignatureToken::U32 => MoveTypeLayout::U32,
        SignatureToken::U64 => MoveTypeLayout::U64,
        SignatureToken::U128 => MoveTypeLayout::U128,
        SignatureToken::U256 => MoveTypeLayout::U256,
        SignatureToken::Address => MoveTypeLayout::Address,
        SignatureToken::Vector(inner) => {
            MoveTypeLayout::Vector(Box::new(primitive_type(view, type_args, inner)?))
        }
        SignatureToken::Datatype(struct_handle_idx) => {
            let resolved_struct = resolve_struct(view, *struct_handle_idx);
            if resolved_struct == RESOLVED_ASCII_STR {
                MoveTypeLayout::Struct(Box::new(move_ascii_str_layout()))
            } else if resolved_struct == RESOLVED_UTF8_STR {
                // both structs structs representing strings have one field - a vector of type u8
                MoveTypeLayout::Struct(Box::new(move_utf8_str_layout()))
            } else if resolved_struct == RESOLVED_SUI_ID {
                MoveTypeLayout::Struct(Box::new(id::ID::layout()))
            } else {
                return None;
            }
        }
        SignatureToken::DatatypeInstantiation(struct_inst) => {
            let (idx, targs) = &**struct_inst;
            let resolved_struct = resolve_struct(view, *idx);
            // is option of a primitive
            if resolved_struct == RESOLVED_STD_OPTION && targs.len() == 1 {
                // there is no MoveLayout for this so the type is not a primitive.
                MoveTypeLayout::Vector(Box::new(primitive_type(view, type_args, &targs[0])?))
            } else {
                return None;
            }
        }
        SignatureToken::TypeParameter(idx) => {
            layout_of_primitive_typetag(type_args.get(*idx as usize)?)?
        }
        SignatureToken::Signer
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => return None,
    })
}

fn layout_of_primitive_typetag(tag: &TypeTag) -> Option<MoveTypeLayout> {
    use MoveTypeLayout as MTL;
    if !is_primitive_type_tag(tag) {
        return None;
    }

    Some(match tag {
        TypeTag::Bool => MTL::Bool,
        TypeTag::U8 => MTL::U8,
        TypeTag::U16 => MTL::U16,
        TypeTag::U32 => MTL::U32,
        TypeTag::U64 => MTL::U64,
        TypeTag::U128 => MTL::U128,
        TypeTag::U256 => MTL::U256,
        TypeTag::Address => MTL::Address,
        TypeTag::Signer => return None,
        TypeTag::Vector(tag) => MTL::Vector(Box::new(layout_of_primitive_typetag(tag)?)),
        TypeTag::Struct(stag) => {
            let StructTag {
                address,
                module,
                name,
                type_params: type_args,
            } = &**stag;
            let resolved_struct = (address, module.as_ident_str(), name.as_ident_str());
            // is id or..
            if resolved_struct == RESOLVED_SUI_ID {
                MTL::Struct(Box::new(id::ID::layout()))
            } else if resolved_struct == RESOLVED_ASCII_STR {
                MTL::Struct(Box::new(move_ascii_str_layout()))
            } else if resolved_struct == RESOLVED_UTF8_STR {
                MTL::Struct(Box::new(move_utf8_str_layout()))
            } else if resolved_struct == RESOLVED_STD_OPTION // is option of a primitive
                && type_args.len() == 1
                && is_primitive_type_tag(&type_args[0])
            {
                MTL::Vector(Box::new(
                    layout_of_primitive_typetag(&type_args[0]).unwrap(),
                ))
            } else {
                return None;
            }
        }
    })
}

fn resolve_object_arg(idx: usize, arg: &JsonValue) -> Result<ObjectID, anyhow::Error> {
    // Every elem has to be a string convertible to a ObjectID
    match arg {
        JsonValue::String(s) => {
            let s = s.trim().to_lowercase();
            if !s.starts_with(HEX_PREFIX) {
                bail!("ObjectID hex string must start with 0x.",);
            }
            Ok(ObjectID::from_hex_literal(&s)?)
        }
        _ => bail!(
            "Unable to parse arg {:?} as ObjectID at pos {}. Expected {:?}-byte hex string \
                prefixed with 0x.",
            arg,
            idx,
            ObjectID::LENGTH,
        ),
    }
}

fn resolve_object_vec_arg(idx: usize, arg: &SuiJsonValue) -> Result<Vec<ObjectID>, anyhow::Error> {
    // Every elem has to be a string convertible to a ObjectID
    match arg.to_json_value() {
        JsonValue::Array(a) => {
            let mut object_ids = vec![];
            for id in a {
                object_ids.push(resolve_object_arg(idx, &id)?);
            }
            Ok(object_ids)
        }
        JsonValue::String(s) if s.starts_with('[') && s.ends_with(']') => {
            // Due to how escaping of square bracket works, we may be dealing with a JSON string
            // representing a JSON array rather than with the array itself ("[0x42,0x7]" rather than
            // [0x42,0x7]).
            let mut object_ids = vec![];
            for tok in s[1..s.len() - 1].split(',') {
                let id = JsonValue::String(tok.to_string());
                object_ids.push(resolve_object_arg(idx, &id)?);
            }
            Ok(object_ids)
        }
        _ => bail!(
            "Unable to parse arg {:?} as vector of ObjectIDs at pos {}. \
             Expected a vector of {:?}-byte hex strings prefixed with 0x.\n\
             Consider escaping your curly braces with a backslash (as in \\[0x42,0x7\\]) \
             or enclosing the whole vector in single quotes (as in '[0x42,0x7]')",
            arg.to_json_value(),
            idx,
            ObjectID::LENGTH,
        ),
    }
}

fn resolve_call_arg(
    view: &CompiledModule,
    type_args: &[TypeTag],
    idx: usize,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<ResolvedCallArg, anyhow::Error> {
    if let Some(layout) = primitive_type(view, type_args, param) {
        return Ok(ResolvedCallArg::Pure(arg.to_bcs_bytes(&layout).map_err(
            |e| {
                anyhow!(
                    "Could not serialize argument of type {:?} at {} into {}. Got error: {:?}",
                    param,
                    idx,
                    layout,
                    e
                )
            },
        )?));
    }

    // in terms of non-primitives we only currently support objects and "flat" (depth == 1) vectors
    // of objects (but not, for example, vectors of references)
    match param {
        SignatureToken::Datatype(_)
        | SignatureToken::DatatypeInstantiation(_)
        | SignatureToken::TypeParameter(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => Ok(ResolvedCallArg::Object(resolve_object_arg(
            idx,
            &arg.to_json_value(),
        )?)),
        SignatureToken::Vector(inner) => match &**inner {
            SignatureToken::Datatype(_) | SignatureToken::DatatypeInstantiation(_) => {
                Ok(ResolvedCallArg::ObjVec(resolve_object_vec_arg(idx, arg)?))
            }
            _ => {
                bail!(
                    "Unexpected non-primitive vector arg {:?} at {} with value {:?}",
                    param,
                    idx,
                    arg
                );
            }
        },
        _ => bail!(
            "Unexpected non-primitive arg {:?} at {} with value {:?}",
            param,
            idx,
            arg
        ),
    }
}

pub fn is_receiving_argument(view: &CompiledModule, arg_type: &SignatureToken) -> bool {
    use SignatureToken as ST;

    // Progress down into references to determine if the underlying type is a receiving
    // type or not.
    let mut token = arg_type;
    while let ST::Reference(inner) | ST::MutableReference(inner) = token {
        token = inner;
    }

    matches!(
        token,
        ST::DatatypeInstantiation(inst) if resolve_struct(view, inst.0) == RESOLVED_RECEIVING_STRUCT && inst.1.len() == 1
    )
}

fn resolve_call_args(
    view: &CompiledModule,
    type_args: &[TypeTag],
    json_args: &[SuiJsonValue],
    parameter_types: &[SignatureToken],
) -> Result<Vec<ResolvedCallArg>, anyhow::Error> {
    json_args
        .iter()
        .zip(parameter_types)
        .enumerate()
        .map(|(idx, (arg, param))| resolve_call_arg(view, type_args, idx, arg, param))
        .collect()
}

/// Resolve the JSON args of a function into the expected formats to make them usable by Move call
/// This is because we have special types which we need to specify in other formats
pub fn resolve_move_function_args(
    package: &MovePackage,
    module_ident: Identifier,
    function: Identifier,
    type_args: &[TypeTag],
    combined_args_json: Vec<SuiJsonValue>,
) -> Result<Vec<(ResolvedCallArg, SignatureToken)>, anyhow::Error> {
    // Extract the expected function signature
    let module = package.deserialize_module(&module_ident, &BinaryConfig::standard())?;
    let function_str = function.as_ident_str();
    let fdef = module
        .function_defs
        .iter()
        .find(|fdef| {
            module.identifier_at(module.function_handle_at(fdef.function).name) == function_str
        })
        .ok_or_else(|| {
            anyhow!(
                "Could not resolve function {} in module {}",
                function,
                module_ident
            )
        })?;
    let function_signature = module.function_handle_at(fdef.function);
    let parameters = &module.signature_at(function_signature.parameters).0;

    // Lengths have to match, less one, due to TxContext
    let expected_len = match parameters.last() {
        Some(param) if TxContext::kind(&module, param) != TxContextKind::None => {
            parameters.len() - 1
        }
        _ => parameters.len(),
    };
    if combined_args_json.len() != expected_len {
        bail!(
            "Expected {} args, found {}",
            expected_len,
            combined_args_json.len()
        );
    }
    // Check that the args are valid and convert to the correct format
    let call_args = resolve_call_args(&module, type_args, &combined_args_json, parameters)?;
    let tupled_call_args = call_args
        .into_iter()
        .zip(parameters.iter())
        .map(|(arg, expected_type)| (arg, expected_type.clone()))
        .collect::<Vec<_>>();
    Ok(tupled_call_args)
}

fn convert_string_to_u256(s: &str) -> Result<U256, anyhow::Error> {
    // Try as normal number
    if let Ok(v) = s.parse::<U256>() {
        return Ok(v);
    }

    // Check prefix
    // For now only Hex supported
    // TODO: add support for bin and octal?

    let s = s.trim().to_lowercase();
    if !s.starts_with(HEX_PREFIX) {
        bail!("Unable to convert {s} to unsigned int.",);
    }
    U256::from_str_radix(s.trim_start_matches(HEX_PREFIX), 16).map_err(|e| e.into())
}

#[macro_export]
macro_rules! call_args {
        ($($value:expr),*) => {
        Ok::<_, anyhow::Error>(vec![$(sui_json::call_arg!($value)?,)*])
    };
    }

#[macro_export]
macro_rules! call_arg {
    ($value:expr) => {{
        use sui_json::SuiJsonValue;
        trait SuiJsonArg {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue>;
        }
        // TODO: anyway to condense this?
        impl SuiJsonArg for &str {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_str(self)
            }
        }
        impl SuiJsonArg for String {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_str(&self)
            }
        }
        impl SuiJsonArg for sui_types::base_types::ObjectID {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_str(&self.to_string())
            }
        }
        impl SuiJsonArg for sui_types::base_types::SuiAddress {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_str(&self.to_string())
            }
        }
        impl SuiJsonArg for u64 {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_bcs_bytes(
                    Some(&sui_json::MoveTypeLayout::U64),
                    &bcs::to_bytes(self)?,
                )
            }
        }
        impl SuiJsonArg for Vec<u8> {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_bcs_bytes(None, &self)
            }
        }
        impl SuiJsonArg for &[u8] {
            fn to_sui_json(&self) -> anyhow::Result<SuiJsonValue> {
                SuiJsonValue::from_bcs_bytes(None, self)
            }
        }
        $value.to_sui_json()
    }};
}

#[macro_export]
macro_rules! type_args {
    ($($value:expr), *) => {{
        use sui_json_rpc_types::SuiTypeTag;
        use sui_types::TypeTag;
        trait SuiJsonTypeArg {
            fn to_sui_json(&self) -> anyhow::Result<SuiTypeTag>;
        }
        impl <T: core::fmt::Display> SuiJsonTypeArg for T {
            fn to_sui_json(&self) -> anyhow::Result<SuiTypeTag> {
                Ok(sui_types::parse_sui_type_tag(&self.to_string())?.into())
            }
        }
        Ok::<_, anyhow::Error>(vec![$($value.to_sui_json()?,)*])
    }};
    }
