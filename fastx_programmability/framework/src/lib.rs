// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastx_types::error::{FastPayError, FastPayResult};
use fastx_verifier::verifier as fastx_bytecode_verifier;
use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, ident_str};
use move_package::BuildConfig;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub mod natives;

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
#[cfg(test)]
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 100_000;

pub fn get_fastx_framework_modules() -> Vec<CompiledModule> {
    let modules = build_framework(".");
    verify_modules(&modules);
    modules
}

pub fn get_move_stdlib_modules() -> Vec<CompiledModule> {
    let denylist = vec![
        ident_str!("Capability").to_owned(),
        ident_str!("Event").to_owned(),
        ident_str!("GUID").to_owned(),
    ];
    let modules: Vec<CompiledModule> = build_framework("deps/move-stdlib")
        .into_iter()
        .filter(|m| !denylist.contains(&m.self_id().name().to_owned()))
        .collect();
    verify_modules(&modules);
    modules
}

/// Given a `path` and a `build_config`, build the package in that path.
/// If we are building the FastX framework, `is_framework` will be true;
/// Otherwise `is_framework` should be false (e.g. calling from client).
pub fn build_move_package(
    path: &Path,
    build_config: BuildConfig,
    is_framework: bool,
) -> FastPayResult<Vec<CompiledModule>> {
    match build_config.compile_package(path, &mut Vec::new()) {
        Err(error) => Err(FastPayError::ModuleBuildFailure {
            error: error.to_string(),
        }),
        Ok(package) => {
            let compiled_modules = package.compiled_modules();
            if !is_framework {
                if let Some(m) = compiled_modules
                    .iter_modules()
                    .iter()
                    .find(|m| m.self_id().address() != &AccountAddress::ZERO)
                {
                    return Err(FastPayError::ModulePublishFailure {
                        error: format!(
                            "Modules must all have 0x0 as their addresses. Violated by module {:?}",
                            m.self_id()
                        ),
                    });
                }
            }
            // Collect all module names from the current package to be published.
            // For each transitive dependent module, if they are not to be published,
            // they must have a non-zero address (meaning they are already published on-chain).
            // TODO: Shall we also check if they are really on-chain in the future?
            let self_modules: HashSet<String> = compiled_modules
                .iter_modules()
                .iter()
                .map(|m| m.self_id().name().to_string())
                .collect();
            if let Some(m) = package
                .transitive_compiled_modules()
                .iter_modules()
                .iter()
                .find(|m| {
                    !self_modules.contains(m.self_id().name().as_str())
                        && m.self_id().address() == &AccountAddress::ZERO
                })
            {
                return Err(FastPayError::ModulePublishFailure { error: format!("Denpendent modules must have been published on-chain with non-0 addresses, unlike module {:?}", m.self_id()) });
            }
            Ok(package
                .transitive_compiled_modules()
                .compute_dependency_graph()
                .compute_topological_order()
                .unwrap()
                .filter(|m| self_modules.contains(m.self_id().name().as_str()))
                .cloned()
                .collect())
        }
    }
}

fn verify_modules(modules: &[CompiledModule]) {
    for m in modules {
        move_bytecode_verifier::verify_module(m).unwrap();
        fastx_bytecode_verifier::verify_module(m).unwrap();
    }
    // TODO(https://github.com/MystenLabs/fastnft/issues/69): Run Move linker
}

fn build_framework(sub_dir: &str) -> Vec<CompiledModule> {
    let mut framework_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    framework_dir.push(sub_dir);
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    build_move_package(&framework_dir, build_config, true).unwrap()
}

#[cfg(test)]
fn get_examples() -> Vec<CompiledModule> {
    let mut framework_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    framework_dir.push("../examples/");
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    let modules = build_move_package(&framework_dir, build_config, true).unwrap();
    verify_modules(&modules);
    modules
}

#[test]
fn check_that_move_code_can_be_built_verified_tested() {
    get_fastx_framework_modules();
    get_examples();
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
