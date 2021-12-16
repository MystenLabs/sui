// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastx_types::MOVE_STDLIB_ADDRESS;
use fastx_verifier::verifier as fastx_bytecode_verifier;
use move_binary_format::CompiledModule;
use move_core_types::{ident_str, language_storage::ModuleId};
use move_package::{compilation::compiled_package::CompiledPackage, BuildConfig};
use std::path::PathBuf;

pub mod natives;

/// Return all the modules of the fastX framework and its dependencies in topologically
/// sorted dependency order (leaves first)
pub fn get_framework_modules() -> Result<Vec<CompiledModule>> {
    let include_examples = false;
    let verify = true;
    get_framework_modules_(include_examples, verify)
}

fn get_framework_modules_(include_examples: bool, verify: bool) -> Result<Vec<CompiledModule>> {
    // TODO: prune unused deps from Move stdlib instead of using an explicit denylist.
    // The manually curated list are modules that do not pass the FastX verifier
    let denylist = vec![
        ModuleId::new(MOVE_STDLIB_ADDRESS, ident_str!("Capability").to_owned()),
        ModuleId::new(MOVE_STDLIB_ADDRESS, ident_str!("Event").to_owned()),
        ModuleId::new(MOVE_STDLIB_ADDRESS, ident_str!("GUID").to_owned()),
    ];
    let package = build(include_examples)?;
    let filtered_modules = package
        .transitive_compiled_modules()
        .iter_modules_owned()
        .into_iter()
        .filter(|m| !denylist.contains(&m.self_id()))
        .collect();
    if verify {
        for m in &filtered_modules {
            move_bytecode_verifier::verify_module(m).unwrap();
            fastx_bytecode_verifier::verify_module(m).unwrap();
            // TODO(https://github.com/MystenLabs/fastnft/issues/69): Run Move linker
        }
    }
    Ok(filtered_modules)
}

/// Include the Move package's `example` modules if `include_examples` is true, omits them otherwise
fn build(include_examples: bool) -> Result<CompiledPackage> {
    let framework_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let build_config = BuildConfig {
        dev_mode: include_examples,
        ..Default::default()
    };
    build_config.compile_package(&framework_dir, &mut Vec::new())
}

#[test]
fn check_that_move_code_can_be_built_and_verified() {
    let include_examples = true;
    let verify = true;
    get_framework_modules_(include_examples, verify).unwrap();
}
