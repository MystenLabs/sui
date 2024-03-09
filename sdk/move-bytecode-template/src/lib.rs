// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use move_binary_format::{file_format::StructFieldInformation, CompiledModule};
use move_core_types::identifier::Identifier;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::{prelude::*, JsValue};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen]
/// Get the version of the crate (useful for testing the package).
pub fn version() -> String {
    VERSION.to_string()
}

#[wasm_bindgen]
/// Deserialize the bytecode into a JSON string.
pub fn deserialize(binary: &[u8]) -> Result<JsValue, JsErr> {
    let compiled_module = CompiledModule::deserialize_with_defaults(binary)?;
    Ok(to_value(&compiled_module)?)
}

#[wasm_bindgen]
/// Perform an operation on a bytecode string - deserialize, patch the identifiers
/// and serialize back to a bytecode string.
pub fn update_identifiers(binary: &[u8], map: JsValue) -> Result<Box<[u8]>, JsErr> {
    let updates: HashMap<String, String> = serde_wasm_bindgen::from_value(map)?;
    let mut compiled_module = CompiledModule::deserialize_with_defaults(binary)?;

    // First update the identifiers.
    for ident in compiled_module.identifiers.iter_mut() {
        let old = ident.to_string();
        if let Some(new) = updates.get(&old) {
            *ident = Identifier::new(new.clone()).map_err(|err| JsErr {
                display: format!("{}", err),
                message: err.to_string(),
            })?;
        }
    }

    // Then sort and collect updated indexes.
    let mut indexes: Vec<usize> = (0..compiled_module.identifiers.len()).collect();
    indexes.sort_by_key(|x| &compiled_module.identifiers[*x]);
    compiled_module.identifiers.sort();

    // Then create a function to find the new index of an identifier.
    let find_pos = |a: u16| indexes.iter().position(|x| *x == a as usize).unwrap() as u16;

    // Then update the rest of the struct.
    compiled_module
        .module_handles
        .iter_mut()
        .for_each(|handle| {
            handle.name.0 = find_pos(handle.name.0);
        });

    compiled_module
        .struct_handles
        .iter_mut()
        .for_each(|handle| {
            handle.name.0 = find_pos(handle.name.0);
        });

    compiled_module
        .function_handles
        .iter_mut()
        .for_each(|handle| {
            handle.name.0 = find_pos(handle.name.0);
        });

    compiled_module.struct_defs.iter_mut().for_each(|def| {
        if let StructFieldInformation::Declared(definitions) = &mut def.field_information {
            definitions.iter_mut().for_each(|field| {
                field.name.0 = find_pos(field.name.0);
            });
        }
    });

    let mut binary = Vec::new();
    compiled_module
        .serialize(&mut binary)
        .map_err(|err| JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        })?;

    Ok(binary.into())
}

#[wasm_bindgen]
/// Updates a constant in the constant pool.
pub fn update_constants(
    binary: &[u8],
    new_value: &[u8],
    expected_value: &[u8],
    expected_type: String,
) -> Result<Box<[u8]>, JsErr> {
    let mut compiled_module = CompiledModule::deserialize_with_defaults(&binary)?;

    compiled_module.constant_pool.iter_mut().for_each(|handle| {
        if handle.data == expected_value && expected_type == format!("{:?}", handle.type_) {
            handle.data = new_value.to_vec();
        };
    });

    let mut binary = Vec::new();
    compiled_module
        .serialize(&mut binary)
        .map_err(|err| JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        })?;

    Ok(binary.into())
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
/// A transformed constant from the constant pool.
pub struct Constant {
    type_: String,
    value_bcs: Box<[u8]>,
}

#[wasm_bindgen]
/// Convenience method to analyze the constant pool; returns all constants in order
/// with their type and BCS value. The index can be used to update a constant.
pub fn get_constants(binary: &[u8]) -> Result<JsValue, JsErr> {
    let compiled_module = CompiledModule::deserialize_with_defaults(&binary)?;
    let constants: Vec<Constant> = compiled_module
        .constant_pool
        .into_iter()
        .map(|constant| Constant {
            type_: format!("{:?}", constant.type_),
            value_bcs: constant.data.into(),
        })
        .collect();

    Ok(to_value(&constants)?)
}

#[wasm_bindgen]
/// Serialize the JSON module into a HEX string.
pub fn serialize(json_module: JsValue) -> Result<Box<[u8]>, JsErr> {
    let compiled_module: CompiledModule = from_value(json_module)?;
    let mut binary = Vec::new();
    compiled_module
        .serialize(&mut binary)
        .map_err(|err| JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        })?;

    Ok(binary.into())
}

#[derive(Serialize, Deserialize)]
/// Error type for better JS handling and generalization
/// of Rust / WASM -> JS error conversion.
pub struct JsErr {
    // type_: String,
    message: String,
    display: String,
}

impl Into<JsValue> for JsErr {
    fn into(self) -> JsValue {
        to_value(&self).unwrap()
    }
}

impl<T: std::error::Error> From<T> for JsErr {
    fn from(err: T) -> Self {
        JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        }
    }
}
