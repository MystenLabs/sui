// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::source_map::SourceMap;
use anyhow::{format_err, Result};
use move_ir_types::location::Loc;
use std::{fs::File, io::Read, path::Path};

pub type Error = (Loc, String);
pub type Errors = Vec<Error>;

pub fn source_map_from_file(file_path: &Path) -> Result<SourceMap> {
    let mut bytes = Vec::new();
    File::open(file_path)
        .ok()
        .and_then(|mut file| file.read_to_end(&mut bytes).ok())
        .ok_or_else(|| format_err!("Error while reading in source map information"))?;
    bcs::from_bytes::<SourceMap>(&bytes)
        .map_err(|_| format_err!("Error deserializing into source map"))
}
