// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail};
use fastcrypto::encoding::{Encoding, Hex};
use move_binary_format::{
    access::ModuleAccess, binary_views::BinaryIndexedView, file_format::SignatureToken,
};
use move_core_types::account_address::AccountAddress;
use move_core_types::u256::U256;
use move_core_types::{
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    value::{MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value as JsonValue};
use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::move_package::MovePackage;
use sui_verifier::entry_points_verifier::{
    is_tx_context, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_SUI_ID, RESOLVED_UTF8_STR,
};

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

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum SuiJsonCallArg {
    // Needs to become an Object Ref or Object ID, depending on object type
    Object(ObjectID),
    // pure value, bcs encoded
    Pure(Vec<u8>),
    // a vector of objects
    ObjVec(Vec<ObjectID>),
}

#[derive(Eq, PartialEq, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SuiJsonValue(JsonValue);
impl SuiJsonValue {
    pub fn new(json_value: JsonValue) -> Result<SuiJsonValue, anyhow::Error> {
        match json_value.clone() {
            // No checks needed for Bool and String
            JsonValue::Bool(_) | JsonValue::String(_) => (),
            JsonValue::Number(n) => {
                // Must be castable to u64
                if !n.is_u64() {
                    return Err(anyhow!("{n} not allowed. Number must be unsigned integer"));
                }
            }
            // Must be homogeneous
            JsonValue::Array(a) => {
                // Fail if not homogeneous
                check_valid_homogeneous(&JsonValue::Array(a))?
            }
            _ => bail!("{json_value} not allowed."),
        };
        Ok(Self(json_value))
    }

    pub fn from_object_id(id: ObjectID) -> SuiJsonValue {
        Self(JsonValue::String(id.to_hex_literal()))
    }

    pub fn to_bcs_bytes(&self, ty: &MoveTypeLayout) -> Result<Vec<u8>, anyhow::Error> {
        let move_value = Self::to_move_value(&self.0, ty)?;
        MoveValue::simple_serialize(&move_value)
            .ok_or_else(|| anyhow!("Unable to serialize {:?}. Expected {}", move_value, ty))
    }

    pub fn from_bcs_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        SuiJsonValue::new(try_from_bcs_bytes(bytes)?)
    }

    pub fn to_json_value(&self) -> JsonValue {
        self.0.clone()
    }

    fn handle_inner_struct_layout(
        inner_vec: &[MoveTypeLayout],
        val: &JsonValue,
        ty: &MoveTypeLayout,
        s: &String,
    ) -> Result<MoveValue, anyhow::Error> {
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

        match &inner_vec[0] {
            MoveTypeLayout::Vector(inner) => match **inner {
                MoveTypeLayout::U8 => Ok(MoveValue::Struct(MoveStruct::Runtime(vec![
                    Self::to_move_value(val, &inner_vec[0].clone())?,
                ]))),
                MoveTypeLayout::Address => Ok(MoveValue::Struct(MoveStruct::Runtime(vec![
                    Self::to_move_value(val, &MoveTypeLayout::Address)?,
                ]))),
                _ => bail!(
                    "Cannot convert string arg {s} to {ty} \
                             which is expected to be a struct \
                             with one field of address or u8 vector type"
                ),
            },
            _ => bail!(
                "Cannot convert string arg {s} to {ty} which is expected \
                 to be a struct with one field of a vector type"
            ),
        }
    }

    fn to_move_value(val: &JsonValue, ty: &MoveTypeLayout) -> Result<MoveValue, anyhow::Error> {
        Ok(match (val, ty) {
            // Bool to Bool is simple
            (JsonValue::Bool(b), MoveTypeLayout::Bool) => MoveValue::Bool(*b),

            // In constructor, we have already checked that the JSON number is unsigned int of at most U64
            (JsonValue::Number(n), MoveTypeLayout::U8) => match n.as_u64() {
                Some(x) => MoveValue::U8(u8::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u8 allowed.", n)),
            },
            (JsonValue::Number(n), MoveTypeLayout::U16) => match n.as_u64() {
                Some(x) => MoveValue::U16(u16::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u16 allowed.", n)),
            },
            (JsonValue::Number(n), MoveTypeLayout::U32) => match n.as_u64() {
                Some(x) => MoveValue::U32(u32::try_from(x)?),
                None => return Err(anyhow!("{} is not a valid number. Only u32 allowed.", n)),
            },
            (JsonValue::Number(n), MoveTypeLayout::U64) => match n.as_u64() {
                Some(x) => MoveValue::U64(x),
                None => return Err(anyhow!("{} is not a valid number. Only u64 allowed.", n)),
            },

            // u8, u16, u32, u64, u128, u256 can be encoded as String
            (JsonValue::String(s), MoveTypeLayout::U8) => {
                MoveValue::U8(u8::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U16) => {
                MoveValue::U16(u16::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U32) => {
                MoveValue::U32(u32::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U64) => {
                MoveValue::U64(u64::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U128) => {
                MoveValue::U128(u128::try_from(convert_string_to_u256(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U256) => {
                MoveValue::U256(convert_string_to_u256(s.as_str())?)
            }
            (JsonValue::String(s), MoveTypeLayout::Struct(MoveStructLayout::Runtime(inner))) => {
                Self::handle_inner_struct_layout(inner, val, ty, s)?
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
                        MoveValue::Vector(vec.iter().copied().map(MoveValue::U8).collect())
                    }
                    MoveTypeLayout::Struct(MoveStructLayout::Runtime(inner)) => {
                        Self::handle_inner_struct_layout(inner, val, ty, s)?
                    }
                    _ => bail!("Cannot convert string arg {s} to {ty}"),
                }
            }

            // We have already checked that the array is homogeneous in the constructor
            (JsonValue::Array(a), MoveTypeLayout::Vector(inner)) => {
                // Recursively build an IntermediateValue array
                MoveValue::Vector(
                    a.iter()
                        .map(|i| Self::to_move_value(i, inner))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }

            (JsonValue::String(s), MoveTypeLayout::Address) => {
                let s = s.trim().to_lowercase();
                if !s.starts_with(HEX_PREFIX) {
                    bail!("Address hex string must start with 0x.",);
                }
                let r = SuiAddress::from_str(&s)?;
                MoveValue::Address(r.into())
            }
            _ => bail!("Unexpected arg {val} for expected type {ty}"),
        })
    }
}

impl Debug for SuiJsonValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn try_from_bcs_bytes(bytes: &[u8]) -> Result<JsonValue, anyhow::Error> {
    // Try to deserialize data
    if let Ok(v) = bcs::from_bytes::<String>(bytes) {
        Ok(JsonValue::String(v))
    } else if let Ok(v) = bcs::from_bytes::<AccountAddress>(bytes) {
        Ok(JsonValue::String(v.to_hex_literal()))
    } else if let Ok(v) = bcs::from_bytes::<u8>(bytes) {
        Ok(JsonValue::Number(Number::from(v)))
    } else if let Ok(v) = bcs::from_bytes::<u16>(bytes) {
        Ok(JsonValue::Number(Number::from(v)))
    } else if let Ok(v) = bcs::from_bytes::<u32>(bytes) {
        Ok(JsonValue::Number(Number::from(v)))
    } else if let Ok(v) = bcs::from_bytes::<u64>(bytes) {
        Ok(JsonValue::Number(Number::from(v)))
    } else if let Ok(v) = bcs::from_bytes::<bool>(bytes) {
        Ok(JsonValue::Bool(v))
    } else if let Ok(v) = bcs::from_bytes::<Vec<u64>>(bytes) {
        let v = v
            .into_iter()
            .map(|v| JsonValue::Number(Number::from(v)))
            .collect();
        Ok(JsonValue::Array(v))
    } else if let Ok(v) = bcs::from_bytes::<Vec<String>>(bytes) {
        let v = v.into_iter().map(JsonValue::String).collect();
        Ok(JsonValue::Array(v))
    } else {
        // Fallback to bytearray if fail to deserialize data
        let v = bytes
            .iter()
            .map(|v| JsonValue::Number(Number::from(*v)))
            .collect();
        Ok(JsonValue::Array(v))
    }
}

impl std::str::FromStr for SuiJsonValue {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        // Wrap input with json! if serde_json fails, the failure usually cause by missing quote escapes.
        SuiJsonValue::new(serde_json::from_str(s).or_else(|_| serde_json::from_value(json!(s)))?)
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

fn is_primitive_type_tag(t: &TypeTag) -> bool {
    match t {
        TypeTag::Bool
        | TypeTag::U8
        | TypeTag::U16
        | TypeTag::U32
        | TypeTag::U64
        | TypeTag::U128
        | TypeTag::U256
        | TypeTag::Address => true,
        TypeTag::Vector(inner) => is_primitive_type_tag(inner),
        TypeTag::Struct(StructTag {
            address,
            module,
            name,
            type_params: type_args,
        }) => {
            let resolved_struct = (address, module.as_ident_str(), name.as_ident_str());
            // is id or..
            if resolved_struct == RESOLVED_SUI_ID {
                return true;
            }
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && type_args.len() == 1
                && is_primitive_type_tag(&type_args[0])
        }
        TypeTag::Signer => false,
    }
}

/// Checks if a give SignatureToken represents a primitive type and, if so, returns MoveTypeLayout
/// for this type (if available). The reason we need to return both information about whether a
/// SignatureToken represents a primitive and an Option representing MoveTypeLayout is that there
/// can be signature tokens that represent primitives but that do not have corresponding
/// MoveTypeLayout (e.g., SignatureToken::StructInstantiation).
pub fn primitive_type(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    param: &SignatureToken,
) -> (bool, Option<MoveTypeLayout>) {
    match param {
        SignatureToken::Bool => (true, Some(MoveTypeLayout::Bool)),
        SignatureToken::U8 => (true, Some(MoveTypeLayout::U8)),
        SignatureToken::U16 => (true, Some(MoveTypeLayout::U16)),
        SignatureToken::U32 => (true, Some(MoveTypeLayout::U32)),
        SignatureToken::U64 => (true, Some(MoveTypeLayout::U64)),
        SignatureToken::U128 => (true, Some(MoveTypeLayout::U128)),
        SignatureToken::U256 => (true, Some(MoveTypeLayout::U256)),
        SignatureToken::Address => (true, Some(MoveTypeLayout::Address)),
        SignatureToken::Vector(inner) => {
            let (is_primitive, inner_layout_opt) = primitive_type(view, type_args, inner);
            match inner_layout_opt {
                Some(inner_layout) => (
                    is_primitive,
                    Some(MoveTypeLayout::Vector(Box::new(inner_layout))),
                ),
                None => (is_primitive, None),
            }
        }
        SignatureToken::Struct(struct_handle_idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *struct_handle_idx);
            if resolved_struct == RESOLVED_ASCII_STR || resolved_struct == RESOLVED_UTF8_STR {
                // both structs structs representing strings have one field - a vector of type u8
                (
                    true,
                    Some(MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![
                        MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
                    ]))),
                )
            } else if resolved_struct == RESOLVED_SUI_ID {
                (
                    true,
                    Some(MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![
                        MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Address)),
                    ]))),
                )
            } else {
                (false, None)
            }
        }
        SignatureToken::StructInstantiation(idx, targs) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *idx);
            // is option of a primitive
            if resolved_struct == RESOLVED_STD_OPTION && targs.len() == 1 {
                // there is no MoveLayout for this so while we can still report whether a type
                // is primitive or not, we can't return the layout
                let (is_primitive, _) = primitive_type(view, type_args, &targs[0]);
                (is_primitive, None)
            } else {
                (false, None)
            }
        }

        SignatureToken::TypeParameter(idx) => (
            type_args
                .get(*idx as usize)
                .map(is_primitive_type_tag)
                .unwrap_or(false),
            None,
        ),

        SignatureToken::Signer
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => (false, None),
    }
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
        _ => bail!(
            "Unable to parse arg {:?} as vector of ObjectIDs at pos {}. \
             Expected a vector of {:?}-byte hex strings prefixed with 0x.",
            arg.to_json_value(),
            idx,
            ObjectID::LENGTH,
        ),
    }
}

fn resolve_call_arg(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    idx: usize,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<SuiJsonCallArg, anyhow::Error> {
    let (is_primitive, layout_opt) = primitive_type(view, type_args, param);
    if is_primitive {
        match layout_opt {
            Some(layout) => {
                return Ok(SuiJsonCallArg::Pure(arg.to_bcs_bytes(&layout).map_err(
                    |e| {
                        anyhow!(
                        "Could not serialize argument of type {:?} at {} into {}. Got error: {:?}",
                        param,
                        idx,
                        layout,
                        e
                    )
                    },
                )?))
            }
            None => {
                debug_assert!(
                    false,
                    "Should be unreachable. All primitive type function args \
                     should have a corresponding MoveLayout"
                );
                bail!(
                    "Could not serialize argument of type {:?} at {}",
                    param,
                    idx
                );
            }
        }
    }

    // in terms of non-primitives we only currently support objects and "flat" (depth == 1) vectors
    // of objects (but not, for example, vectors of references)
    match param {
        SignatureToken::Struct(_)
        | SignatureToken::StructInstantiation(_, _)
        | SignatureToken::TypeParameter(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => Ok(SuiJsonCallArg::Object(resolve_object_arg(
            idx,
            &arg.to_json_value(),
        )?)),
        SignatureToken::Vector(inner) => match &**inner {
            SignatureToken::Struct(_) | SignatureToken::StructInstantiation(_, _) => {
                Ok(SuiJsonCallArg::ObjVec(resolve_object_vec_arg(idx, arg)?))
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

fn resolve_call_args(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    json_args: &[SuiJsonValue],
    parameter_types: &[SignatureToken],
) -> Result<Vec<SuiJsonCallArg>, anyhow::Error> {
    json_args
        .iter()
        .zip(parameter_types)
        .enumerate()
        .map(|(idx, (arg, param))| resolve_call_arg(view, type_args, idx, arg, param))
        .collect()
}

/// Resolve a the JSON args of a function into the expected formats to make them usable by Move call
/// This is because we have special types which we need to specify in other formats
pub fn resolve_move_function_args(
    package: &MovePackage,
    module_ident: Identifier,
    function: Identifier,
    type_args: &[TypeTag],
    combined_args_json: Vec<SuiJsonValue>,
) -> Result<Vec<SuiJsonCallArg>, anyhow::Error> {
    // Extract the expected function signature
    let module = package.deserialize_module(&module_ident)?;
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

    if !fdef.is_entry {
        bail!(
            "{}::{} does not have public(script) visibility",
            module.self_id(),
            function,
        )
    }

    let view = BinaryIndexedView::Module(&module);

    // Lengths have to match, less one, due to TxContext
    let expected_len = match parameters.last() {
        Some(param) if is_tx_context(&view, param) => parameters.len() - 1,
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
    resolve_call_args(&view, type_args, &combined_args_json, parameters)
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
