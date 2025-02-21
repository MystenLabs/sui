// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::source_map::SourceMap;
use anyhow::{format_err, Result};
use move_ir_types::location::Loc;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub type Error = (Loc, String);
pub type Errors = Vec<Error>;

pub fn source_map_from_file(file_path: &Path) -> Result<SourceMap> {
    if file_path.extension().is_some_and(|ext| ext == "json") {
        return deserialize_from_json(file_path);
    }
    let mut bytes = Vec::new();
    File::open(file_path)
        .ok()
        .and_then(|mut file| file.read_to_end(&mut bytes).ok())
        .ok_or_else(|| format_err!("Error while reading in source map information"))?;
    bcs::from_bytes::<SourceMap>(&bytes)
        .map_err(|_| format_err!("Error deserializing into source map"))
}

pub fn serialize_to_json_string(map: &SourceMap) -> Result<String> {
    serde_json::to_string_pretty(map).map_err(|e| format_err!("Error serializing to json: {}", e))
}

pub fn serialize_to_json(map: &SourceMap) -> Result<Vec<u8>> {
    serde_json::to_vec(map).map_err(|e| format_err!("Error serializing to json: {}", e))
}

pub fn serialize_to_json_file(map: &SourceMap, file_path: &Path) -> Result<()> {
    let json = serialize_to_json_string(map)?;
    let mut f =
        std::fs::File::create(file_path).map_err(|e| format_err!("Error creating file: {}", e))?;
    f.write_all(json.as_bytes())
        .map_err(|e| format_err!("Error writing to file: {}", e))?;
    Ok(())
}

pub fn deserialize_from_json(file_path: &Path) -> Result<SourceMap> {
    let mut file = File::open(file_path).map_err(|e| format_err!("Error opening file: {}", e))?;
    let mut json = String::new();
    file.read_to_string(&mut json)
        .map_err(|e| format_err!("Error reading file: {}", e))?;
    serde_json::from_str(&json).map_err(|e| format_err!("Error deserializing from json: {}", e))
}

pub fn convert_to_json(file_path: &Path) -> Result<()> {
    let map = source_map_from_file(file_path)?;
    let json_file_path = file_path.with_extension("json");
    serialize_to_json_file(&map, &json_file_path)
}
