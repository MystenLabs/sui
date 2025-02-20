// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_cli::base::test::UnitTestResult;
use move_package::LintFlag;
use move_unit_test::UnitTestingConfig;
use sui_move::unit_test::run_move_unit_tests;
use sui_move_build::BuildConfig;

pub(crate) const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
pub(crate) const FRAMEWORK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/sui-framework/packages"
);

/// Ensure packages build outside of test mode.
pub(crate) fn build(path: &Path) -> datatest_stable::Result<()> {
    let Some(path) = path.parent() else {
        panic!("No parent for Move.toml file at: {}", path.display());
    };

    let mut config = BuildConfig::new_for_testing();
    config.config.dev_mode = true;
    config.run_bytecode_verifier = true;
    config.print_diags_to_stderr = true;
    config.config.warnings_are_errors = true;
    config.config.silence_warnings = false;
    config.config.lint_flag = LintFlag::LEVEL_DEFAULT;

    config
        .build(path)
        .unwrap_or_else(|e| panic!("Building package {}.\nWith error {e}", path.display()));

    Ok(())
}

/// Ensure package sbuild under test mode and all the tests pass.
pub(crate) fn tests(path: &Path) -> datatest_stable::Result<()> {
    let Some(path) = path.parent() else {
        panic!("No parent for Move.toml file at: {}", path.display());
    };

    let mut config = BuildConfig::new_for_testing();

    config.config.dev_mode = true;
    config.config.test_mode = true;
    config.run_bytecode_verifier = true;
    config.print_diags_to_stderr = true;
    config.config.warnings_are_errors = true;
    config.config.silence_warnings = false;
    config.config.lint_flag = LintFlag::LEVEL_DEFAULT;

    let move_config = config.config.clone();
    // TODO: Remove this when we support per-test gas limits.
    let mut testing_config = UnitTestingConfig::default_with_bound(Some(3_000_000));
    testing_config.filter = std::env::var("FILTER").ok().map(|s| s.to_string());

    assert_eq!(
        run_move_unit_tests(path, move_config, Some(testing_config), false, false).unwrap(),
        UnitTestResult::Success
    );

    Ok(())
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
