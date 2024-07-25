// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_cli::base::test::UnitTestResult;
use move_package::LintFlag;
use move_unit_test::UnitTestingConfig;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use sui_move::unit_test::run_move_unit_tests;
use sui_move_build::BuildConfig;

const FILTER_ENV: &str = "FILTER";

#[test]
#[cfg_attr(msim, ignore)]
fn run_move_stdlib_unit_tests() {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.extend(["..", "sui-framework", "packages", "move-stdlib"]);
    check_move_unit_tests(&buf);
}

#[test]
#[cfg_attr(msim, ignore)]
fn run_sui_framework_tests() {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.extend(["..", "sui-framework", "packages", "sui-framework"]);
    check_move_unit_tests(&buf);
}

#[test]
#[cfg_attr(msim, ignore)]
fn run_sui_system_tests() {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.extend(["..", "sui-framework", "packages", "sui-system"]);
    check_move_unit_tests(&buf);
}

#[test]
#[cfg_attr(msim, ignore)]
fn run_deepbook_tests() {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.extend(["..", "sui-framework", "packages", "deepbook"]);
    check_move_unit_tests(&buf);
}
#[test]
#[cfg_attr(msim, ignore)]
fn run_bridge_tests() {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.extend(["..", "sui-framework", "packages", "bridge"]);
    check_move_unit_tests(&buf);
}

fn check_packages_recursively(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry?;
        if entry.path().join("Move.toml").exists() {
            check_package_builds(&entry.path());
            check_move_unit_tests(&entry.path());
        } else if entry.file_type()?.is_dir() {
            check_packages_recursively(&entry.path())?;
        }
    }
    Ok(())
}

#[test]
#[cfg_attr(msim, ignore)]
fn run_examples_move_unit_tests() -> io::Result<()> {
    let examples = {
        let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        buf.extend(["..", "..", "examples"]);
        buf
    };

    check_packages_recursively(&examples)?;

    Ok(())
}

/// Ensure packages build outside of test mode.
fn check_package_builds(path: &Path) {
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
}

fn check_move_unit_tests(path: &Path) {
    let mut config = BuildConfig::new_for_testing();
    // Make sure to verify tests
    config.config.dev_mode = true;
    config.config.test_mode = true;
    config.run_bytecode_verifier = true;
    config.print_diags_to_stderr = true;
    config.config.warnings_are_errors = true;
    config.config.silence_warnings = false;
    config.config.lint_flag = LintFlag::LEVEL_DEFAULT;
    let move_config = config.config.clone();
    let mut testing_config = UnitTestingConfig::default_with_bound(Some(3_000_000));
    testing_config.filter = std::env::var(FILTER_ENV).ok().map(|s| s.to_string());

    assert_eq!(
        run_move_unit_tests(path, move_config, Some(testing_config), false).unwrap(),
        UnitTestResult::Success
    );
}
