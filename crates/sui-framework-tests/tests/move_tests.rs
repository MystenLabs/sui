// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_cli::base::test::UnitTestResult;
use move_package_alt_compilation::lint_flag::LintFlag;
use move_unit_test::UnitTestingConfig;
use sui_framework_tests::setup_examples;
use sui_move::unit_test::{MAX_UNIT_TEST_INSTRUCTIONS, run_move_unit_tests};
use sui_move_build::BuildConfig;
use sui_package_alt::SuiFlavor;

pub(crate) const EXAMPLES: &str = "../../examples";
pub(crate) const FRAMEWORK: &str = "../sui-framework/packages";

// `oracle-adapter/move` pins the Pyth Sui contracts to commit `62c7a5b` (the
// resolved tip of the frozen `sui-contract-testnet` branch), which it must, to
// match the deployed Testnet package `0xabf837...`. That revision's `price.move`
// has an unattached doc comment (a stray `//` splits a `///` block), and this
// harness builds with `warnings_are_errors`, so the dependency's warning fails
// the build. The package is verified locally instead: `sui move build` is clean
// and `sui move test` passes 13/13.
//
// Re-inclusion condition: once Pyth ships a warning-clean revision that is still
// published-at `0xabf837...` (the doc-comment fix is commit `c0e3b9f`,
// pyth-network/pyth-crosschain#3165), pin that revision and remove this
// exclusion. The fix cannot be pinned today: the fix commit pulls a Wormhole rev
// that double-defines the `wormhole` address (unresolvable by consumer
// overrides), and later `main` uses the new package format an old-style manifest
// cannot depend on.
#[cfg(not(msim))]
const DIRS_TO_EXCLUDE: &[&str] = &["oracle-adapter/move"];
/// We cannot support packages that depend on git dependencies on simtests.
/// TODO: we probably also shouldn't be doing these in normal CI, since generally having CI depend
/// on other git repos is frowned upon
#[cfg(msim)]
const DIRS_TO_EXCLUDE: &[&str] = &["nft-rental", "usdc_usage", "oracle-adapter/move"];

/// Ensure packages build outside of test mode.
#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
pub(crate) async fn build(path: &Path) -> datatest_stable::Result<()> {
    println!("{path:?}");
    let Some(path) = path.parent() else {
        panic!("No parent for Move.toml file at: {}", path.display());
    };
    if should_exclude_dir(path) {
        return Ok(());
    }

    // TODO dvx-1889: this is kind of hacky - we intentionally unclobber the lockfile because we
    // don't want them to change. Update this when we properly implement install_dir
    let tempdir = setup_examples();
    let path = tempdir.path().join("crates/sui-framework").join(path);

    let mut config = BuildConfig::new_for_testing();
    config.run_bytecode_verifier = true;
    config.print_diags_to_stderr = true;
    config.config.warnings_are_errors = true;
    config.config.silence_warnings = false;
    config.config.lint_flag = LintFlag::LEVEL_DEFAULT;

    config
        .build_async(&path)
        .await
        .unwrap_or_else(|e| panic!("Building package {}.\nWith error {e}", path.display()));

    Ok(())
}

#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
/// Ensure package sbuild under test mode and all the tests pass.
pub(crate) async fn tests(path: &Path) -> datatest_stable::Result<()> {
    let Some(path) = path.parent() else {
        panic!("No parent for Move.toml file at: {}", path.display());
    };

    if should_exclude_dir(path) {
        return Ok(());
    }

    let mut config = BuildConfig::new_for_testing();

    config.config.test_mode = true;
    config.run_bytecode_verifier = true;
    config.print_diags_to_stderr = true;
    config.config.warnings_are_errors = true;
    config.config.silence_warnings = false;
    config.config.lint_flag = LintFlag::LEVEL_DEFAULT;

    let move_config = config.config.clone();
    // TODO: Remove this when we support per-test gas limits.
    let mut testing_config =
        UnitTestingConfig::default_with_bound(Some(*MAX_UNIT_TEST_INSTRUCTIONS));
    testing_config.filter = std::env::var("FILTER").ok().map(|s| s.to_string());

    assert_eq!(
        run_move_unit_tests(
            path,
            move_config,
            Some(testing_config),
            false,
            false,
            SuiFlavor::new()
        )
        .await
        .unwrap(),
        UnitTestResult::Success
    );

    Ok(())
}

/// On simtests, we exclude dirs that depend on external (git)
/// dependencies.
fn should_exclude_dir(path: &Path) -> bool {
    for exclude_dir in DIRS_TO_EXCLUDE {
        if path
            .to_str()
            .unwrap()
            .ends_with(format!("/{}", exclude_dir).as_str())
        {
            return true;
        }
    }

    false
}

datatest_stable::harness!(
    build,
    EXAMPLES,
    r".*/Move.toml$",
    tests,
    EXAMPLES,
    r".*/Move.toml$",
    build,
    FRAMEWORK,
    r".*/Move.toml$",
    tests,
    FRAMEWORK,
    r".*/Move.toml$",
);
