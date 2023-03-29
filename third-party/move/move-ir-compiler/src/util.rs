// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use move_binary_format::file_format::{CompiledModule, CompiledScript};
use move_bytecode_source_map::source_map::SourceMap;
use move_ir_to_bytecode::{
    compiler::{compile_module, compile_script},
    parser::{parse_module, parse_script},
};
use std::{fs, path::Path};

pub fn do_compile_script(
    source_path: &Path,
    dependencies: &[CompiledModule],
) -> (CompiledScript, SourceMap) {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("Unable to read file: {:?}", source_path))
        .unwrap();
    let parsed_script = parse_script(&source).unwrap();
    compile_script(parsed_script, dependencies).unwrap()
}

pub fn do_compile_module(
    source_path: &Path,
    dependencies: &[CompiledModule],
) -> (CompiledModule, SourceMap) {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("Unable to read file: {:?}", source_path))
        .unwrap();
    let parsed_module = parse_module(&source).unwrap();
    compile_module(parsed_module, dependencies).unwrap()
}
