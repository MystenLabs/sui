// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_cli::base::test::UnitTestResult;
use move_core_types::gas_algebra::InternalGas;
use move_package::BuildConfig as MoveBuildConfig;
use move_unit_test::{extensions::set_extension_hook, UnitTestingConfig};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_test_utils::gas_schedule::INITIAL_COST_SCHEDULE;
use natives::object_runtime::ObjectRuntime;
use once_cell::sync::Lazy;
use std::{collections::BTreeMap, path::Path};
use sui_framework_build::compiled_package::{BuildConfig, CompiledPackage};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TransactionDigest, error::SuiResult, in_memory_storage::InMemoryStorage,
    messages::InputObjects, temporary_store::TemporaryStore, MOVE_STDLIB_ADDRESS,
    SUI_FRAMEWORK_ADDRESS,
};

pub mod cost_calib;
pub mod natives;

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

static SUI_FRAMEWORK_TEST: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    const SUI_FRAMEWORK_BYTES: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/sui-framework-test"));

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

static MOVE_STDLIB_TEST: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    const MOVE_STDLIB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/move-stdlib-test"));

    let serialized_modules: Vec<Vec<u8>> = bcs::from_bytes(MOVE_STDLIB_BYTES).unwrap();

    serialized_modules
        .into_iter()
        .map(|module| CompiledModule::deserialize(&module).unwrap())
        .collect()
});

static SET_EXTENSION_HOOK: Lazy<()> =
    Lazy::new(|| set_extension_hook(Box::new(new_testing_object_runtime)));

fn new_testing_object_runtime(ext: &mut NativeContextExtensions) {
    let store = InMemoryStorage::new(vec![]);
    let state_view = TemporaryStore::new(
        store,
        InputObjects::new(vec![]),
        TransactionDigest::random(),
        &ProtocolConfig::get_for_min_version(),
    );
    ext.add(ObjectRuntime::new(
        Box::new(state_view),
        BTreeMap::new(),
        false,
        &ProtocolConfig::get_for_min_version(),
    ))
}

pub fn get_sui_framework() -> Vec<CompiledModule> {
    Lazy::force(&SUI_FRAMEWORK).to_owned()
}

pub fn get_sui_framework_test() -> Vec<CompiledModule> {
    Lazy::force(&SUI_FRAMEWORK_TEST).to_owned()
}

pub fn get_move_stdlib() -> Vec<CompiledModule> {
    Lazy::force(&MOVE_STDLIB).to_owned()
}

pub fn get_move_stdlib_test() -> Vec<CompiledModule> {
    Lazy::force(&MOVE_STDLIB_TEST).to_owned()
}

pub const DEFAULT_FRAMEWORK_PATH: &str = env!("CARGO_MANIFEST_DIR");

// TODO: remove these in favor of new costs
pub fn legacy_test_cost() -> InternalGas {
    InternalGas::new(0)
}

pub fn legacy_emit_cost() -> InternalGas {
    InternalGas::new(52)
}

pub fn legacy_create_signer_cost() -> InternalGas {
    InternalGas::new(24)
}

pub fn legacy_empty_cost() -> InternalGas {
    InternalGas::new(84)
}

pub fn legacy_length_cost() -> InternalGas {
    InternalGas::new(98)
}

/// This function returns a result of UnitTestResult. The outer result indicates whether it
/// successfully started running the test, and the inner result indicatests whether all tests pass.
pub fn run_move_unit_tests(
    path: &Path,
    build_config: MoveBuildConfig,
    config: Option<UnitTestingConfig>,
    compute_coverage: bool,
) -> anyhow::Result<UnitTestResult> {
    // bind the extension hook if it has not yet been done
    Lazy::force(&SET_EXTENSION_HOOK);

    let config = config
        .unwrap_or_else(|| UnitTestingConfig::default_with_bound(Some(MAX_UNIT_TEST_INSTRUCTIONS)));

    move_cli::base::test::run_move_unit_tests(
        path,
        build_config,
        UnitTestingConfig {
            report_stacktrace_on_abort: true,
            ..config
        },
        natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS),
        Some(INITIAL_COST_SCHEDULE.clone()),
        compute_coverage,
        &mut std::io::stdout(),
    )
}

/// Wrapper of the build command that verifies the framework version. Should eventually be removed once we can
/// do this in the obvious way (via version checks)
pub fn build_move_package(path: &Path, config: BuildConfig) -> SuiResult<CompiledPackage> {
    //let test_mode = config.config.test_mode;
    let pkg = config.build(path.to_path_buf())?;
    /*if test_mode {
        pkg.verify_framework_version(get_sui_framework_test(), get_move_stdlib_test())?;
    } else {
        pkg.verify_framework_version(get_sui_framework(), get_move_stdlib())?;
    }*/
    Ok(pkg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_framework_move_unit_tests() {
        get_sui_framework();
        get_move_stdlib();
        let path = PathBuf::from(DEFAULT_FRAMEWORK_PATH);
        BuildConfig::new_for_testing().build(path.clone()).unwrap();
        check_move_unit_tests(&path);
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_examples_move_unit_tests() {
        let examples = vec![
            "basics",
            "defi",
            "capy",
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
            BuildConfig::new_for_testing().build(path.clone()).unwrap();
            check_move_unit_tests(&path);
        }
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_book_examples_move_unit_tests() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../doc/book/examples");

        BuildConfig::new_for_testing().build(path.clone()).unwrap();
        check_move_unit_tests(&path);
    }

    fn check_move_unit_tests(path: &Path) {
        // build tests first to enable Sui-specific test code verification
        matches!(
            build_move_package(
                path,
                BuildConfig {
                    config: MoveBuildConfig {
                        test_mode: true, // make sure to verify tests
                        ..MoveBuildConfig::default()
                    },
                    run_bytecode_verifier: true,
                    print_diags_to_stderr: true,
                },
            ),
            Ok(_)
        );
        assert_eq!(
            run_move_unit_tests(path, MoveBuildConfig::default(), None, false).unwrap(),
            UnitTestResult::Success
        );
    }
}
