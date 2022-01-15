// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastx_verifier::verifier as fastx_bytecode_verifier;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use move_package::BuildConfig;
use std::path::PathBuf;

pub mod natives;

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
#[cfg(test)]
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 100_000;

pub fn get_fastx_framework_modules() -> Vec<CompiledModule> {
    let modules = build(".");
    veirfy_modules(&modules);
    modules
}

pub fn get_move_stdlib_modules() -> Vec<CompiledModule> {
    let denylist = vec![
        ident_str!("Capability").to_owned(),
        ident_str!("Event").to_owned(),
        ident_str!("GUID").to_owned(),
    ];
    let modules: Vec<CompiledModule> = build("deps/move-stdlib")
        .into_iter()
        .filter(|m| !denylist.contains(&m.self_id().name().to_owned()))
        .collect();
    veirfy_modules(&modules);
    modules
}

fn veirfy_modules(modules: &[CompiledModule]) {
    for m in modules {
        move_bytecode_verifier::verify_module(m).unwrap();
        fastx_bytecode_verifier::verify_module(m).unwrap();
        // TODO(https://github.com/MystenLabs/fastnft/issues/69): Run Move linker
    }
}

fn build(sub_dir: &str) -> Vec<CompiledModule> {
    let mut framework_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    framework_dir.push(sub_dir);
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    build_config
        .compile_package(&framework_dir, &mut Vec::new())
        .unwrap()
        .compiled_modules()
        .iter_modules_owned()
}

#[test]
fn check_that_move_code_can_be_built_verified_tested() {
    get_fastx_framework_modules();
}

#[test]
fn run_move_unit_tests() {
    use fastx_types::{FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS};
    use move_cli::package::cli::{self, UnitTestResult};

    use move_unit_test::UnitTestingConfig;
    use std::path::Path;

    let framework_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let result = cli::run_move_unit_tests(
        framework_dir,
        BuildConfig {
            dev_mode: false,
            ..Default::default()
        },
        UnitTestingConfig::default_with_bound(Some(MAX_UNIT_TEST_INSTRUCTIONS)),
        natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS),
        /* compute_coverage */ false,
    )
    .unwrap();
    assert!(result == UnitTestResult::Success);
}
