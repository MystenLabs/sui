// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_bytecode_utils::Modules;
use move_cli::base::test::UnitTestResult;
use move_core_types::gas_algebra::InternalGas;
use move_package::{compilation::compiled_package::CompiledPackage, BuildConfig};
use move_unit_test::{extensions::set_extension_hook, UnitTestingConfig};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use natives::object_runtime::ObjectRuntime;
use once_cell::sync::Lazy;
use std::{collections::BTreeMap, path::Path};
use sui_types::{
    base_types::TransactionDigest,
    error::{SuiError, SuiResult},
    in_memory_storage::InMemoryStorage,
    messages::InputObjects,
    temporary_store::TemporaryStore,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

pub mod cost_calib;
pub mod natives;

pub use sui_framework_build::build_move_stdlib_modules as get_move_stdlib_modules;
pub use sui_framework_build::verify_modules;
use sui_framework_build::{build_move_package_with_deps, filter_package_modules};
use sui_types::sui_serde::{Base64, Encoding};

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

static SET_EXTENSION_HOOK: Lazy<()> =
    Lazy::new(|| set_extension_hook(Box::new(new_testing_object_runtime)));

fn new_testing_object_runtime(ext: &mut NativeContextExtensions) {
    let store = InMemoryStorage::new(vec![]);
    let state_view = TemporaryStore::new(
        store,
        InputObjects::new(vec![]),
        TransactionDigest::random(),
    );
    ext.add(ObjectRuntime::new(Box::new(state_view), BTreeMap::new()))
}

pub fn get_sui_framework() -> Vec<CompiledModule> {
    Lazy::force(&SUI_FRAMEWORK).to_owned()
}

pub fn get_move_stdlib() -> Vec<CompiledModule> {
    Lazy::force(&MOVE_STDLIB).to_owned()
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

/// Given a `path` and a `build_config`, build the package in that path and return the compiled modules as base64.
/// This is useful for when publishing via JSON
pub fn build_move_package_to_base64(
    path: &Path,
    build_config: BuildConfig,
) -> Result<Vec<String>, SuiError> {
    build_move_package_to_bytes(path, build_config)
        .map(|mods| mods.iter().map(Base64::encode).collect::<Vec<_>>())
}

/// Given a `path` and a `build_config`, build the package in that path and return the compiled modules as Vec<Vec<u8>>.
/// This is useful for when publishing
pub fn build_move_package_to_bytes(
    path: &Path,
    build_config: BuildConfig,
) -> Result<Vec<Vec<u8>>, SuiError> {
    build_move_package(path, build_config).map(|mods| {
        mods.iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize(&mut bytes).unwrap();
                bytes
            })
            .collect::<Vec<_>>()
    })
}

pub fn build_and_verify_package(
    path: &Path,
    build_config: BuildConfig,
) -> SuiResult<Vec<CompiledModule>> {
    let modules = build_move_package(path, build_config)?;
    verify_modules(&modules)?;
    Ok(modules)
}

pub fn run_move_unit_tests(
    path: &Path,
    build_config: BuildConfig,
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
        compute_coverage,
        &mut std::io::stdout(),
    )
}

pub fn build_move_package(
    path: &Path,
    build_config: BuildConfig,
) -> SuiResult<Vec<CompiledModule>> {
    let pkg = build_move_package_with_deps(path, build_config)?;
    verify_framework_version(&pkg)?;
    filter_package_modules(&pkg)
}

/// Version of the framework code that the binary used for compilation expects should be the same as
/// version of the framework code bundled as compiled package's dependency and this function
/// verifies this.
fn verify_framework_version(pkg: &CompiledPackage) -> SuiResult<()> {
    // We stash compiled modules in the Modules map which is sorted so that we can compare sets of
    // compiled modules directly.

    let dep_framework_modules = pkg.all_modules_map().iter_modules_owned();
    let dep_framework: Vec<&CompiledModule> = dep_framework_modules
        .iter()
        .filter(|m| *m.self_id().address() == SUI_FRAMEWORK_ADDRESS)
        .collect();

    let framework_modules = Modules::new(get_sui_framework().iter()).iter_modules_owned();
    let framework: Vec<&CompiledModule> = framework_modules.iter().collect();

    // compare framework modules pulled as dependencies (if any - a developer may choose to use only
    // stdlib) with framework modules bundled with the distribution
    if !dep_framework.is_empty() && dep_framework != framework {
        // note: this advice is overfitted to the most common failure modes we see:
        // user is trying to publish to testnet, but has a `sui` binary and Sui Framework
        // sources that are not in sync. the first part of the advice ensures that the
        // user's project is always pointing at the devnet copy of the `Sui` Framework.
        // the second ensures that the `sui` binary matches the devnet framework
        return Err(SuiError::ModuleVerificationFailure {
            error: "Sui framework version mismatch detected.\
		    Make sure that you are using a GitHub dep in your Move.toml:\
		    \
                    [dependencies]
                    Sui = { git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework\", rev = \"devnet\" }
`                   \
                    If that does not fix the issue, your `sui` binary is likely out of date--try \
                    cargo install --locked --git https://github.com/MystenLabs/sui.git --branch devnet sui"
                .to_string(),
        });
    }

    let dep_stdlib_modules = pkg.all_modules_map().iter_modules_owned();
    let dep_stdlib: Vec<&CompiledModule> = dep_stdlib_modules
        .iter()
        .filter(|m| *m.self_id().address() == MOVE_STDLIB_ADDRESS)
        .collect();

    let stdlib_modules = Modules::new(get_move_stdlib().iter()).iter_modules_owned();
    let stdlib: Vec<&CompiledModule> = stdlib_modules.iter().collect();

    // compare stdlib modules pulled as dependencies (if any) with stdlib modules bundled with the
    // distribution
    if !dep_stdlib.is_empty() && dep_stdlib != stdlib {
        return Err(SuiError::ModuleVerificationFailure {
            error: "Move stdlib version mismatch detected.\
                    Make sure that the sui command line tool and the Move standard library code\
                    used as a dependency correspond to the same git commit"
                .to_string(),
        });
    }

    Ok(())
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
        build_and_verify_package(
            &PathBuf::from(DEFAULT_FRAMEWORK_PATH),
            BuildConfig::default(),
        )
        .unwrap();
        run_move_unit_tests(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            BuildConfig::default(),
            None,
            false,
        )
        .unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
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
            build_and_verify_package(&path, BuildConfig::default()).unwrap();
            run_move_unit_tests(&path, BuildConfig::default(), None, false).unwrap();
        }
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_book_examples_move_unit_tests() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../doc/book/examples");

        build_and_verify_package(&path, BuildConfig::default()).unwrap();
        run_move_unit_tests(&path, BuildConfig::default(), None, false).unwrap();
    }
}
