// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use anyhow::{anyhow, bail};
// Alias the type names for clarity
use move_binary_format::{
    access::ModuleAccess,
    file_format::{SignatureToken, Visibility},
};
use move_core_types::{
    identifier::Identifier,
    value::{MoveTypeLayout, MoveValue},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use sui_types::base_types::{decode_bytes_hex, ObjectID, SuiAddress};
use sui_types::object::Object;

const HEX_PREFIX: &str = "0x";

#[cfg(test)]
#[path = "unit_tests/sui_json.rs"]
mod base_types_tests;

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum SuiJsonCallArg {
    // Needs to become an Object Ref or Object ID, depending on object type
    Object(ObjectID),
    // pure value, bcs encoded
    Pure(Vec<u8>),
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, JsonSchema)]
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
                    return Err(anyhow!("Arrays must be homogeneous",));
                }
            }
            _ => return Err(anyhow!("{json_value} not allowed.")),
        };
        Ok(Self(json_value))
    }

    pub fn to_bcs_bytes(&self, ty: &MoveTypeLayout) -> Result<Vec<u8>, anyhow::Error> {
        let move_value = Self::to_move_value(&self.0, ty)?;
        MoveValue::simple_serialize(&move_value)
            .ok_or_else(|| anyhow!("Unable to serialize {:?}. Expected {}", move_value, ty))
    }

    pub fn to_json_value(&self) -> JsonValue {
        self.0.clone()
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

            // U256 Not allowed for now

            // We can encode U8 Vector as string in 2 ways
            // 1. If it starts with 0x, we treat it as hex strings, where each pair is a byte
            // 2. If it does not start with 0x, we treat each character as an ASCII encoded byte
            // We have to support both for the convenience of the user. This is because sometime we need Strings as arg
            // Other times we need vec of hex bytes for address. Issue is both Address and Strings are represented as Vec<u8> in Move call
            (JsonValue::String(s), MoveTypeLayout::Vector(t)) => {
                if !matches!(&**t, &MoveTypeLayout::U8) {
                    return Err(anyhow!("Cannot convert string arg {s} to {ty}"));
                }
                let vec = if s.starts_with(HEX_PREFIX) {
                    // If starts with 0x, treat as hex vector
                    hex::decode(s.trim_start_matches(HEX_PREFIX))?
                } else {
                    // Else raw bytes
                    s.as_bytes().to_vec()
                };
                MoveValue::Vector(vec.iter().copied().map(MoveValue::U8).collect())
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
                    return Err(anyhow!("Address hex string must start with 0x.",));
                }
                let r: SuiAddress = decode_bytes_hex(s.trim_start_matches(HEX_PREFIX))?;
                MoveValue::Address(r.into())
            }
            _ => return Err(anyhow!("Unexpected arg {val} for expected type {ty}")),
        })
    }
}

impl std::str::FromStr for SuiJsonValue {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        SuiJsonValue::new(serde_json::from_str(s)?)
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

fn resolve_primtive_arg(
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<Vec<u8>, anyhow::Error> {
    let move_type_layout = make_prim_move_type_layout(param)?;
    // Check that the args are what we expect or can be converted
    // Then return the serialized bcs value
    match arg.to_bcs_bytes(&move_type_layout) {
        Ok(a) => Ok(a),
        Err(e) => Err(anyhow!(
            "Unable to parse arg at type {}. Got error: {:?}",
            move_type_layout,
            e
        )),
    }
}

fn make_prim_move_type_layout(param: &SignatureToken) -> Result<MoveTypeLayout, anyhow::Error> {
    Ok(match param {
        SignatureToken::Bool => MoveTypeLayout::Bool,
        SignatureToken::U8 => MoveTypeLayout::U8,
        SignatureToken::U64 => MoveTypeLayout::U64,
        SignatureToken::U128 => MoveTypeLayout::U128,
        SignatureToken::Address => MoveTypeLayout::Address,
        SignatureToken::Signer => MoveTypeLayout::Signer,
        SignatureToken::Vector(inner) => {
            MoveTypeLayout::Vector(Box::new(make_prim_move_type_layout(inner)?))
        }
        SignatureToken::Struct(_)
        | SignatureToken::StructInstantiation(_, _)
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

fn resolve_object_arg(idx: usize, arg: &SuiJsonValue) -> Result<ObjectID, anyhow::Error> {
    // Every elem has to be a string convertible to a ObjectID
    match arg.to_json_value() {
        JsonValue::String(s) => {
            let s = s.trim().to_lowercase();
            if !s.starts_with(HEX_PREFIX) {
                return Err(anyhow!("ObjectID hex string must start with 0x.",));
            }
            Ok(ObjectID::from_hex_literal(&s)?)
        }
        _ => Err(anyhow!(
            "Unable to parse arg {:?} as ObjectID at pos {}. Expected {:?} byte hex string \
                prefixed with 0x.",
            ObjectID::LENGTH,
            idx,
            arg.to_json_value(),
        )),
    }
}

fn resolve_call_arg(
    idx: usize,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<SuiJsonCallArg, anyhow::Error> {
    Ok(match param {
        SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::Address
        | SignatureToken::Vector(_) => SuiJsonCallArg::Pure(resolve_primtive_arg(arg, param)?),

        SignatureToken::Struct(_)
        | SignatureToken::StructInstantiation(_, _)
        | SignatureToken::TypeParameter(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => {
            SuiJsonCallArg::Object(resolve_object_arg(idx, arg)?)
        }

        SignatureToken::Signer => unreachable!(),
    })
}

fn resolve_call_args(
    json_args: &[SuiJsonValue],
    parameter_types: &[SignatureToken],
) -> Result<Vec<SuiJsonCallArg>, anyhow::Error> {
    json_args
        .iter()
        .zip(parameter_types)
        .enumerate()
        .map(|(idx, (arg, param))| resolve_call_arg(idx, arg, param))
        .collect()
}

/// Resolve a the JSON args of a function into the expected formats to make them usable by Move call
/// This is because we have special types which we need to specify in other formats
pub fn resolve_move_function_args(
    package: &Object,
    module_ident: Identifier,
    function: Identifier,
    combined_args_json: Vec<SuiJsonValue>,
) -> Result<Vec<SuiJsonCallArg>, anyhow::Error> {
    // Extract the expected function signature
    let module = package
        .data
        .try_as_package()
        .ok_or_else(|| anyhow!("Cannot get package from object"))?
        .deserialize_module(&module_ident)?;
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

    if fdef.visibility != Visibility::Script {
        bail!(
            "{}::{} does not have public(script) visibility",
            module.self_id(),
            function,
        )
    }

    // Lengths have to match, less one, due to TxContext
    let expected_len = parameters.len() - 1;
    if combined_args_json.len() != expected_len {
        return Err(anyhow!(
            "Expected {} args, found {}",
            expected_len,
            combined_args_json.len()
        ));
    }

    // Check that the args are valid and convert to the correct format
    resolve_call_args(&combined_args_json, parameters)
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
        return Err(anyhow!("Unable to convert {s} to unsigned int.",));
    }
    u128::from_str_radix(s.trim_start_matches(HEX_PREFIX), 16).map_err(|e| e.into())
}
