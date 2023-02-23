// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::build;
use clap::Parser;
use move_cli::base::{
    self,
    test::{self, UnitTestResult},
};
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Test {
    #[clap(flatten)]
    pub test: test::Test,
}
impl Test {
    pub fn execute(
        &self,
        path: Option<PathBuf>,
        build_config: BuildConfig,
        unit_test_config: UnitTestingConfig,
    ) -> anyhow::Result<UnitTestResult> {
        // find manifest file directory from a given path or (if missing) from current dir
        let rerooted_path = base::reroot_path(path)?;
        // pre build for Sui-specific verifications
        let with_unpublished_deps = false;
        let dump_bytecode_as_base64 = false;
        let generate_struct_layouts: bool = false;
        build::Build::execute_internal(
            &rerooted_path,
            BuildConfig {
                test_mode: true, // make sure to verify tests
                ..build_config.clone()
            },
            with_unpublished_deps,
            dump_bytecode_as_base64,
            generate_struct_layouts,
        )?;
        sui_framework::run_move_unit_tests(
            &rerooted_path,
            build_config,
            Some(unit_test_config),
            self.test.compute_coverage,
        )
    }
}
