// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::{file_format::CompiledModule, file_format_common::VERSION_MAX};
use move_compiler::{compiled_unit::AnnotatedCompiledUnit, Compiler as MoveCompiler};
use std::{fs::File, io::Write, path::Path};
use tempfile::tempdir;

pub fn compile_units(s: &str) -> Result<Vec<AnnotatedCompiledUnit>> {
    let dir = tempdir()?;

    let file_path = dir.path().join("modules.move");
    {
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", s)?;
    }

    let (_, units) = MoveCompiler::from_files(
        None,
        vec![file_path.to_str().unwrap().to_string()],
        vec![],
        move_stdlib::move_stdlib_named_addresses(),
    )
    .build_and_report()?;

    dir.close()?;

    Ok(units)
}

pub fn expect_modules(
    units: impl IntoIterator<Item = AnnotatedCompiledUnit>,
) -> impl Iterator<Item = CompiledModule> {
    units
        .into_iter()
        .map(|annot_module| annot_module.named_module.module)
}

pub fn compile_modules_in_file(path: &Path) -> Result<Vec<CompiledModule>> {
    let (_, units) = MoveCompiler::from_files(
        None,
        vec![path.to_str().unwrap().to_string()],
        vec![],
        std::collections::BTreeMap::<String, _>::new(),
    )
    .build_and_report()?;

    Ok(expect_modules(units).collect())
}

#[allow(dead_code)]
pub fn compile_modules(s: &str) -> Result<Vec<CompiledModule>> {
    Ok(expect_modules(compile_units(s)?).collect())
}

pub fn as_module(unit: AnnotatedCompiledUnit) -> CompiledModule {
    unit.named_module.module
}

pub fn serialize_module_at_max_version(
    module: &CompiledModule,
    binary: &mut Vec<u8>,
) -> Result<()> {
    module.serialize_with_version(VERSION_MAX, binary)
}
