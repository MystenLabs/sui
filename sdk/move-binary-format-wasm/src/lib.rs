// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use move_binary_format::{file_format::StructFieldInformation, CompiledModule};
use move_core_types::identifier::Identifier;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::{prelude::*, JsValue};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen]
/// Get the version of the crate (useful for testing the package).
pub fn version() -> String {
    VERSION.to_string()
}

#[wasm_bindgen]
/// Deserialize the bytecode into a JSON string.
pub fn deserialize(binary: String) -> Result<JsValue, JsErr> {
    let bytes = hex::decode(binary)?;
    let compiled_module = CompiledModule::deserialize_with_defaults(&bytes[..])?;
    let serialized = serde_json::to_string(&compiled_module)?;
    Ok(to_value(&serialized)?)
}

#[wasm_bindgen]
/// Perform an operation on a bytecode string - deserialize, patch the identifiers
/// and serialize back to a bytecode string.
pub fn update_identifiers(binary: String, map: JsValue) -> Result<JsValue, JsErr> {
    let bytes = hex::decode(binary)?;
    let updates: HashMap<String, String> = serde_wasm_bindgen::from_value(map)?;
    let mut compiled_module = CompiledModule::deserialize_with_defaults(&bytes[..])?;

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
        def.struct_handle.0 = find_pos(def.struct_handle.0);
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

    Ok(to_value(&hex::encode(binary))?)
}

#[wasm_bindgen]
/// Serialize the JSON module into a HEX string.
pub fn serialize(json_module: String) -> Result<JsValue, JsErr> {
    let compiled_module: CompiledModule = serde_json::from_str(json_module.as_str())?;
    let mut binary = Vec::new();
    compiled_module
        .serialize(&mut binary)
        .map_err(|err| JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        })?;
    Ok(to_value(&hex::encode(binary))?)
}

#[derive(Serialize, Deserialize)]
/// Error type for better JS handling and generalization
/// of Rust / WASM -> JS error conversion.
pub struct JsErr {
    // type_: String,
    message: String,
    display: String,
}

impl<T: std::error::Error> From<T> for JsErr {
    fn from(err: T) -> Self {
        JsErr {
            display: format!("{}", err),
            message: err.to_string(),
        }
    }
}

impl From<JsErr> for JsValue {
    fn from(err: JsErr) -> Self {
        to_value(&err).unwrap()
    }
}
