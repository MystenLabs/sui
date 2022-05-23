// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_compiler::compiled_unit::{CompiledUnit, NamedCompiledModule};
use move_core_types::{account_address::AccountAddress, ident_str, language_storage::ModuleId};
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use num_enum::TryFromPrimitive;
use std::{collections::HashSet, path::Path};
use sui_types::error::{SuiError, SuiResult};
use sui_verifier::verifier as sui_bytecode_verifier;

#[cfg(test)]
use std::path::PathBuf;

pub mod natives;

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 100_000;

pub const DEFAULT_FRAMEWORK_PATH: &str = env!("CARGO_MANIFEST_DIR");

#[derive(TryFromPrimitive, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    /// System event: transfer between addresses
    TransferToAddress,
    /// System event: transfer object to another object
    TransferToObject,
    /// System event: freeze object
    FreezeObject,
    /// System event: turn an object into a shared object
    ShareObject,
    /// System event: an object ID is deleted. This does not necessarily
    /// mean an object is being deleted. However whenever an object is being
    /// deleted, the object ID must be deleted and this event will be
    /// emitted.
    DeleteObjectID,
    /// System event: a child object is deleted along with a child ref.
    DeleteChildObject,
    /// User-defined event
    User,
}

pub fn get_sui_framework_modules(lib_dir: &Path) -> SuiResult<Vec<CompiledModule>> {
    let modules = build_framework(lib_dir)?;
    verify_modules(&modules)?;
    Ok(modules)
}

pub fn get_move_stdlib_modules(lib_dir: &Path) -> SuiResult<Vec<CompiledModule>> {
    let denylist = vec![
        ident_str!("Capability").to_owned(),
        ident_str!("Event").to_owned(),
        ident_str!("GUID").to_owned(),
        #[cfg(not(test))]
        ident_str!("Debug").to_owned(),
    ];
    let modules: Vec<CompiledModule> = build_framework(lib_dir)?
        .into_iter()
        .filter(|m| !denylist.contains(&m.self_id().name().to_owned()))
        .collect();
    verify_modules(&modules)?;
    Ok(modules)
}

/// Given a `path` and a `build_config`, build the package in that path and return the compiled modules as base64.
/// This is useful for when publishing via JSON
/// If we are building the Sui framework, `is_framework` will be true;
/// Otherwise `is_framework` should be false (e.g. calling from client).
pub fn build_move_package_to_base64(
    path: &Path,
    is_framework: bool,
) -> Result<Vec<String>, SuiError> {
    build_move_package_to_bytes(path, is_framework)
        .map(|mods| mods.iter().map(base64::encode).collect::<Vec<_>>())
}

/// Given a `path` and a `build_config`, build the package in that path and return the compiled modules as Vec<Vec<u8>>.
/// This is useful for when publishing
/// If we are building the Sui framework, `is_framework` will be true;
/// Otherwise `is_framework` should be false (e.g. calling from client).
pub fn build_move_package_to_bytes(
    path: &Path,
    is_framework: bool,
) -> Result<Vec<Vec<u8>>, SuiError> {
    build_move_package(
        path,
        BuildConfig {
            ..Default::default()
        },
        is_framework,
    )
    .map(|mods| {
        mods.iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize(&mut bytes).unwrap();
                bytes
            })
            .collect::<Vec<_>>()
    })
}

/// Given a `path` and a `build_config`, build the package in that path.
/// If we are building the Sui framework, `is_framework` will be true;
/// Otherwise `is_framework` should be false (e.g. calling from client).
pub fn build_move_package(
    path: &Path,
    build_config: BuildConfig,
    is_framework: bool,
) -> SuiResult<Vec<CompiledModule>> {
    match build_config.compile_package(path, &mut Vec::new()) {
        Err(error) => Err(SuiError::ModuleBuildFailure {
            error: error.to_string(),
        }),
        Ok(package) => {
            let compiled_modules = package.root_modules_map();
            if !is_framework {
                if let Some(m) = compiled_modules
                    .iter_modules()
                    .iter()
                    .find(|m| m.self_id().address() != &AccountAddress::ZERO)
                {
                    return Err(SuiError::ModulePublishFailure {
                        error: format!(
                            "Modules must all have 0x0 as their addresses. Violated by module {:?}",
                            m.self_id()
                        ),
                    });
                }
            }
            // Collect all module IDs from the current package to be
            // published (module names are not sufficient as we may
            // have modules with the same names in user code and in
            // Sui framework which would result in the latter being
            // pulled into a set of modules to be published).
            // For each transitive dependent module, if they are not to be published,
            // they must have a non-zero address (meaning they are already published on-chain).
            // TODO: Shall we also check if they are really on-chain in the future?
            let self_modules: HashSet<ModuleId> = compiled_modules
                .iter_modules()
                .iter()
                .map(|m| m.self_id())
                .collect();
            if let Some(m) =
                package
                    .deps_compiled_units
                    .iter()
                    .find_map(|(_, unit)| match &unit.unit {
                        CompiledUnit::Module(NamedCompiledModule { module: m, .. })
                            if !self_modules.contains(&m.self_id())
                                && m.self_id().address() == &AccountAddress::ZERO =>
                        {
                            Some(m)
                        }
                        _ => None,
                    })
            {
                return Err(SuiError::ModulePublishFailure { error: format!("Dependent modules must have been published on-chain with non-0 addresses, unlike module {:?}", m.self_id()) });
            }
            Ok(package
                .all_modules_map()
                .compute_dependency_graph()
                .compute_topological_order()
                .unwrap()
                .filter(|m| self_modules.contains(&m.self_id()))
                .cloned()
                .collect())
        }
    }
}

pub fn build_and_verify_user_package(path: &Path) -> SuiResult<Vec<CompiledModule>> {
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    let modules = build_move_package(path, build_config, false)?;
    verify_modules(&modules)?;
    Ok(modules)
}

fn verify_modules(modules: &[CompiledModule]) -> SuiResult {
    for m in modules {
        move_bytecode_verifier::verify_module(m).map_err(|err| {
            SuiError::ModuleVerificationFailure {
                error: err.to_string(),
            }
        })?;
        sui_bytecode_verifier::verify_module(m)?;
    }
    Ok(())
    // TODO(https://github.com/MystenLabs/sui/issues/69): Run Move linker
}

fn build_framework(framework_dir: &Path) -> SuiResult<Vec<CompiledModule>> {
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    build_move_package(framework_dir, build_config, true)
}

pub fn run_move_unit_tests(path: &Path, config: Option<UnitTestingConfig>) -> SuiResult {
    use move_cli::package::cli::{self, UnitTestResult};
    use sui_types::{MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS};

    let config = config
        .unwrap_or_else(|| UnitTestingConfig::default_with_bound(Some(MAX_UNIT_TEST_INSTRUCTIONS)));

    let result = cli::run_move_unit_tests(
        path,
        BuildConfig::default(),
        UnitTestingConfig {
            report_stacktrace_on_abort: true,
            instruction_execution_bound: MAX_UNIT_TEST_INSTRUCTIONS,
            ..config
        },
        natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS),
        /* compute_coverage */ false,
    )
    .map_err(|err| SuiError::MoveUnitTestFailure {
        error: format!("{:?}", err),
    })?;
    if result == UnitTestResult::Failure {
        Err(SuiError::MoveUnitTestFailure {
            error: "Test failed".to_string(),
        })
    } else {
        Ok(())
    }
}

#[test]
fn run_framework_move_unit_tests() {
    get_sui_framework_modules(&PathBuf::from(DEFAULT_FRAMEWORK_PATH)).unwrap();
    run_move_unit_tests(Path::new(env!("CARGO_MANIFEST_DIR")), None).unwrap();
}

#[test]
fn run_examples_move_unit_tests() {
    let examples = vec![
        "basics",
        "defi",
        "fungible_tokens",
        "games",
        "nfts",
        "objects_tutorial",
    ];
    for example in examples {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sui_programmability/examples")
            .join(example);
        build_and_verify_user_package(&path).unwrap();
        run_move_unit_tests(&path, None).unwrap();
    }
}
