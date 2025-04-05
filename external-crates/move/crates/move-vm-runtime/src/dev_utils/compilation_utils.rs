// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{dev_utils::storage::StoredPackage, shared::types::OriginalId};
use anyhow::Result;
use move_binary_format::{file_format::CompiledModule, file_format_common::VERSION_MAX};
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::warning_filters::WarningFiltersBuilder,
    editions::{Edition, Flavor},
    shared::{NumericalAddress, PackageConfig},
    Compiler as MoveCompiler,
};
use std::{collections::BTreeMap, fs::File, io::Write, path::PathBuf};
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
        [("std", NumericalAddress::parse_str("0x1").unwrap())]
            .into_iter()
            .collect(),
    )
    .build_and_report()?;

    dir.close()?;

    Ok(units)
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

pub fn make_base_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src");
    path.push("unit_tests");
    path.push("move_packages");
    path
}

pub fn expect_modules(
    units: impl IntoIterator<Item = AnnotatedCompiledUnit>,
) -> impl Iterator<Item = CompiledModule> {
    units
        .into_iter()
        .map(|annot_module| annot_module.named_module.module)
}

pub fn compile_packages_in_file(filename: &str, dependencies: &[&str]) -> Vec<StoredPackage> {
    let mut path = make_base_path();
    path.push(filename);
    let deps = dependencies
        .iter()
        .map(|dep| {
            let mut path = make_base_path();
            path.push(dep);
            path.to_string_lossy().to_string()
        })
        .collect();
    let (_, units) = MoveCompiler::from_files(
        None,
        vec![path.to_str().unwrap().to_string()],
        deps,
        std::collections::BTreeMap::<String, _>::new(),
    )
    .set_default_config(PackageConfig {
        is_dependency: false,
        warning_filter: WarningFiltersBuilder::unused_warnings_filter_for_test(),
        flavor: Flavor::Sui,
        edition: Edition::E2024_ALPHA,
    })
    .build_and_report()
    .expect("Failed module compilation");

    let modules = expect_modules(units).collect::<Vec<_>>();
    let mut packages = BTreeMap::new();
    for module in modules {
        let module_id = module.self_id();
        packages
            .entry(*module_id.address())
            .or_insert_with(Vec::new)
            .push(module);
    }
    // NB: storage id == runtime id for these packages
    packages
        .into_iter()
        .map(|(id, modules)| StoredPackage::from_modules_for_testing(id, modules).unwrap())
        .collect()
}

pub fn compile_packages(
    filename: &str,
    dependencies: &[&str],
) -> BTreeMap<OriginalId, StoredPackage> {
    compile_packages_in_file(filename, dependencies)
        .into_iter()
        .map(|p| (p.original_id, p))
        .collect()
}
