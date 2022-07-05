// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::build;
use anyhow::ensure;
use move_cli::package::cli::UnitTestResult;
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use std::path::Path;

pub fn execute(
    path: &Path,
    dump_bytecode_as_base64: bool,
    build_config: BuildConfig,
    unit_test_config: UnitTestingConfig,
    compute_coverage: bool,
) -> anyhow::Result<UnitTestResult> {
    ensure!(
        !dump_bytecode_as_base64,
        "dump-bytecode-as-base64 is meaningless for unit tests"
    );
    // pre build for Sui-specific verifications
    build::execute(path, false, build_config.clone())?;
    sui_framework::run_move_unit_tests(path, build_config, Some(unit_test_config), compute_coverage)
}
