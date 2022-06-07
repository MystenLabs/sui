// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use num_enum::TryFromPrimitive;
use once_cell::sync::Lazy;
use std::path::Path;
use sui_types::error::{SuiError, SuiResult};

pub mod natives;

pub use sui_framework_build::build_move_stdlib_modules as get_move_stdlib_modules;
pub use sui_framework_build::build_sui_framework_modules as get_sui_framework_modules;
pub use sui_framework_build::{build_framework, build_move_package, verify_modules};

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 100_000;

static SUI_FRAMEWORK: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    const SUI_FRAMEWORK_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sui-framework"));

    let serialized_modules: Vec<Vec<u8>> = bcs::from_bytes(SUI_FRAMEWORK_BYTES).unwrap();

    serialized_modules
        .into_iter()
        .map(|module| CompiledModule::deserialize(&module).unwrap())
        .collect()
});

static MOVE_STDLIB: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    const MOVE_STDLIB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/move-stdlib"));

    let serialized_modules: Vec<Vec<u8>> = bcs::from_bytes(MOVE_STDLIB_BYTES).unwrap();

    serialized_modules
        .into_iter()
        .map(|module| CompiledModule::deserialize(&module).unwrap())
        .collect()
});

pub fn get_sui_framework() -> Vec<CompiledModule> {
    Lazy::force(&SUI_FRAMEWORK).to_owned()
}

pub fn get_move_stdlib() -> Vec<CompiledModule> {
    Lazy::force(&MOVE_STDLIB).to_owned()
}

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

pub fn build_and_verify_user_package(path: &Path) -> SuiResult<Vec<CompiledModule>> {
    let build_config = BuildConfig {
        dev_mode: false,
        ..Default::default()
    };
    let modules = build_move_package(path, build_config, false)?;
    verify_modules(&modules)?;
    Ok(modules)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn run_framework_move_unit_tests() {
        get_sui_framework();
        get_move_stdlib();
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
            "move_tutorial",
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
}
