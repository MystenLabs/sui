// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use wasm_bindgen::{prelude::*, JsValue};
use serde::{Serialize, Deserialize};
use serde_wasm_bindgen::to_value;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen]
/// Get the version of the crate (useful for testing the package).
pub fn version() -> String {
    VERSION.to_string()
}

#[wasm_bindgen]
/// Deserialize the bytecode into a JSON string.
pub fn deserialize(binary: String) -> Result<JsValue, JsErr> {
    let bytes = hex::decode(&binary)?;
    let compiled_module = CompiledModule::deserialize(&bytes[..])?;
    let serialized = serde_json::to_string(&compiled_module)?;
    Ok(to_value(&serialized)?)
}

#[wasm_bindgen]
/// Serialize the JSON module into a HEX string.
pub fn serialize(json_module: String) -> Result<JsValue, JsErr> {
    let compiled_module: CompiledModule = serde_json::from_str(json_module.as_str())?;
    let mut binary = Vec::new();
    compiled_module.serialize(&mut binary).map_err(|err| JsErr {
        display: format!("{}", err),
        message: err.to_string()
    })?;
    Ok(to_value(&hex::encode(&binary))?)
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
            message: err.to_string()
        }
    }
}

impl Into<JsValue> for JsErr {
    fn into(self) -> JsValue {
        to_value(&self).unwrap()
    }
}
