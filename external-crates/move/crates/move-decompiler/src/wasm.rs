// Copyright (c) Verichains, 2023

use crate::decompiler::{Decompiler, OptimizerSettings};
use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{CompiledModule, CompiledScript},
};

#[derive(Serialize, Deserialize)]
pub struct WasmOptimizerSettings {
    pub disable_optimize_variables_declaration: bool,
}

fn hex_decode(hex_str: &str) -> Result<Vec<u8>, String> {
    if hex_str.len() % 2 != 0 {
        return Err("Input string must have an even length".to_string());
    }

    let mut bytes = Vec::with_capacity(hex_str.len() / 2);

    for chunk in hex_str.as_bytes().chunks(2) {
        let hex_pair = std::str::from_utf8(chunk)
            .map_err(|_| "Invalid UTF-8 sequence in input".to_string())?;
        let byte = u8::from_str_radix(hex_pair, 16)
            .map_err(|_| "Invalid hex character".to_string())?;
        bytes.push(byte);
    }

    Ok(bytes)
}

#[wasm_bindgen]
pub fn decompile_modules(
    modules_bytecode: JsValue,
    settings: JsValue,
) -> Result<JsValue, JsValue> {
    let modules_bytecode: Vec<String> = serde_wasm_bindgen::from_value(modules_bytecode).map_err(|e| e.to_string())?;
    let settings: WasmOptimizerSettings = serde_wasm_bindgen::from_value(settings).map_err(|e| e.to_string())?;
    let binaries_store = modules_bytecode.iter().map(|bytecode| {
        let bytecode_bytes = hex_decode(bytecode)?;
        CompiledModule::deserialize(&bytecode_bytes).map_err(|e| e.to_string())
    }).collect::<Result<Vec<_>, String>>()?;
    let binaries: Vec<_> = binaries_store.iter().map(|m| BinaryIndexedView::Module(m)).collect();
    let mut decompiler = Decompiler::new(
        binaries,
        OptimizerSettings {
            disable_optimize_variables_declaration: settings.disable_optimize_variables_declaration,
        }
    );
    let source = decompiler.decompile().expect("Error: unable to decompile");
    Ok(JsValue::from_str(&source))
}
