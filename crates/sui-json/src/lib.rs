// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail};
use move_binary_format::{
    access::ModuleAccess, binary_views::BinaryIndexedView, file_format::SignatureToken,
};
use move_core_types::account_address::AccountAddress;
use move_core_types::{
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    value::{MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value as JsonValue};
use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};
use sui_types::base_types::{decode_bytes_hex, ObjectID, SuiAddress};
use sui_types::move_package::MovePackage;
use sui_verifier::entry_points_verifier::{
    is_tx_context, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_SUI_ID, RESOLVED_UTF8_STR,
};

const HEX_PREFIX: &str = "0x";

#[cfg(test)]
mod tests;

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
                if !is_homogeneous(&JsonValue::Array(a)) {
                    bail!("Arrays must be homogeneous",);
                }
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
                MoveTypeLayout::U8 | MoveTypeLayout::Address => {
                    Ok(MoveValue::Struct(MoveStruct::Runtime(vec![
                        Self::to_move_value(val, &inner_vec[0].clone())?,
                    ])))
                }
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
            // Hence it is okay to unwrap() numbers
            (JsonValue::Number(n), MoveTypeLayout::U8) => {
                MoveValue::U8(u8::try_from(n.as_u64().unwrap())?)
            }
            (JsonValue::Number(n), MoveTypeLayout::U64) => MoveValue::U64(n.as_u64().unwrap()),

            // u8, u64, u128 can be encoded as String
            (JsonValue::String(s), MoveTypeLayout::U8) => {
                MoveValue::U8(u8::try_from(convert_string_to_u128(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U64) => {
                MoveValue::U64(u64::try_from(convert_string_to_u128(s.as_str())?)?)
            }
            (JsonValue::String(s), MoveTypeLayout::U128) => {
                MoveValue::U128(convert_string_to_u128(s.as_str())?)
            }
            (JsonValue::String(s), MoveTypeLayout::Struct(MoveStructLayout::Runtime(inner))) => {
                Self::handle_inner_struct_layout(inner, val, ty, s)?
            }
            (JsonValue::String(s), MoveTypeLayout::Vector(t)) => {
                match &**t {
                    MoveTypeLayout::U8 => {
                        // U256 Not allowed for now

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
                            hex::decode(s.trim_start_matches(HEX_PREFIX))?
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
                let r: SuiAddress = decode_bytes_hex(&s)?;
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
/// The invariant is that all types at a given level must be the same or be empty
pub fn is_homogeneous(val: &JsonValue) -> bool {
    let mut deq: VecDeque<&JsonValue> = VecDeque::new();
    deq.push_back(val);
    is_homogeneous_rec(&mut deq)
}

/// Check via BFS
/// The invariant is that all types at a given level must be the same or be empty
fn is_homogeneous_rec(curr_q: &mut VecDeque<&JsonValue>) -> bool {
    if curr_q.is_empty() {
        // Nothing to do
        return true;
    }
    // Queue for the next level
    let mut next_q = VecDeque::new();
    // The types at this level must be the same
    let mut level_type = ValidJsonType::Any;

    // Process all in this queue/level
    while !curr_q.is_empty() {
        // Okay to unwrap since we know values exist
        let curr = match curr_q.pop_front().unwrap() {
            JsonValue::Bool(_) => ValidJsonType::Bool,
            JsonValue::Number(_) => ValidJsonType::Number,
            JsonValue::String(_) => ValidJsonType::String,
            JsonValue::Array(w) => {
                // Add to the next level
                w.iter().for_each(|t| next_q.push_back(t));
                ValidJsonType::Array
            }
            // Not valid
            _ => return false,
        };

        if level_type == ValidJsonType::Any {
            // Update the level with the first found type
            level_type = curr;
        } else if level_type != curr {
            // Mismatch in the level
            return false;
        }
    }
    // Process the next level
    is_homogeneous_rec(&mut next_q)
}

fn resolve_primitive_arg(
    view: &BinaryIndexedView,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<Vec<u8>, anyhow::Error> {
    let move_type_layout = make_prim_move_type_layout(view, param)?;
    // Check that the args are what we expect or can be converted
    // Then return the serialized bcs value
    arg.to_bcs_bytes(&move_type_layout).map_err(|e| {
        anyhow!(
            "Unable to parse arg at type {}. Got error: {:?}",
            move_type_layout,
            e
        )
    })
}

pub fn make_prim_move_type_layout(
    view: &BinaryIndexedView,
    param: &SignatureToken,
) -> Result<MoveTypeLayout, anyhow::Error> {
    Ok(match param {
        SignatureToken::Bool => MoveTypeLayout::Bool,
        SignatureToken::U8 => MoveTypeLayout::U8,
        SignatureToken::U64 => MoveTypeLayout::U64,
        SignatureToken::U128 => MoveTypeLayout::U128,
        SignatureToken::Address => MoveTypeLayout::Address,
        SignatureToken::Signer => MoveTypeLayout::Signer,
        SignatureToken::Vector(inner) => {
            MoveTypeLayout::Vector(Box::new(make_prim_move_type_layout(view, inner)?))
        }
        SignatureToken::Struct(struct_handle_idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *struct_handle_idx);
            if resolved_struct == RESOLVED_ASCII_STR || resolved_struct == RESOLVED_UTF8_STR {
                // both structs structs representing strings have one field - a vector of type u8
                MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![MoveTypeLayout::Vector(
                    Box::new(MoveTypeLayout::U8),
                )]))
            } else if resolved_struct == RESOLVED_SUI_ID {
                MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![MoveTypeLayout::Vector(
                    Box::new(MoveTypeLayout::Address),
                )]))
            } else {
                bail!(
                    "Could not serialize argument of struct type {:?} \
                       (only the following structs are currently supported: \
                       object::ID, ascii::String and staring::String)",
                    param
                )
            }
        }
        SignatureToken::StructInstantiation(_, _)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::TypeParameter(_) => {
            debug_assert!(
                false,
                "Should be unreachable. Args should be primitive types only"
            );
            bail!("Could not serialize argument of type {:?}", param)
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
        _ => bail!(
            "Unable to parse arg {:?} as vector of ObjectIDs at pos {}. \
             Expected a vector of {:?}-byte hex strings prefixed with 0x.",
            arg.to_json_value(),
            idx,
            ObjectID::LENGTH,
        ),
    }
}

fn is_primitive_type_tag(t: &TypeTag) -> bool {
    match t {
        TypeTag::Bool | TypeTag::U8 | TypeTag::U64 | TypeTag::U128 | TypeTag::Address => true,
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

pub fn is_primitive(view: &BinaryIndexedView, type_args: &[TypeTag], t: &SignatureToken) -> bool {
    match t {
        SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::Address => true,

        SignatureToken::Struct(idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *idx);
            // is ID
            resolved_struct == RESOLVED_SUI_ID
                || resolved_struct == RESOLVED_ASCII_STR
                || resolved_struct == RESOLVED_UTF8_STR
        }

        SignatureToken::StructInstantiation(idx, targs) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *idx);
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && targs.len() == 1
                && is_primitive(view, type_args, &targs[0])
        }
        SignatureToken::Vector(inner) => is_primitive(view, type_args, inner),

        SignatureToken::TypeParameter(idx) => type_args
            .get(*idx as usize)
            .map(is_primitive_type_tag)
            .unwrap_or(false),

        SignatureToken::Signer
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => false,
    }
}

fn resolve_call_arg(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    idx: usize,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<SuiJsonCallArg, anyhow::Error> {
    if is_primitive(view, type_args, param) {
        return Ok(SuiJsonCallArg::Pure(resolve_primitive_arg(
            view, arg, param,
        )?));
    }
    // in terms of non-primitives we only currently support objects and "flat" (depth == 1) vectors
    // of objects (but not, for example, vectors of references)
    match param {
        SignatureToken::Struct(_) => Ok(SuiJsonCallArg::Object(resolve_object_arg(
            idx,
            &arg.to_json_value(),
        )?)),
        SignatureToken::Vector(inner) => {
            if let SignatureToken::Struct(_) = &**inner {
                Ok(SuiJsonCallArg::ObjVec(resolve_object_vec_arg(idx, arg)?))
            } else {
                bail!(
                    "Unexpected non-primitive vector arg {:?} at {} with value {:?}",
                    param,
                    idx,
                    arg
                );
            }
        }
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

fn convert_string_to_u128(s: &str) -> Result<u128, anyhow::Error> {
    // Try as normal number
    if let Ok(v) = s.parse::<u128>() {
        return Ok(v);
    }

    // Check prefix
    // For now only Hex supported
    // TODO: add support for bin and octal?

    let s = s.trim().to_lowercase();
    if !s.starts_with(HEX_PREFIX) {
        bail!("Unable to convert {s} to unsigned int.",);
    }
    u128::from_str_radix(s.trim_start_matches(HEX_PREFIX), 16).map_err(|e| e.into())
}
