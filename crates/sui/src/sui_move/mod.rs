// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::test::UnitTestResult;
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;

pub mod build;
pub mod new;
pub mod unit_test;

#[derive(Parser)]
pub enum Command {
    Build(build::Build),
    New(new::New),
    Test(unit_test::Test),
}

pub fn execute_move_command(
    package_path: Option<PathBuf>,
    build_config: BuildConfig,
    command: Command,
) -> anyhow::Result<()> {
    match command {
        Command::Build(c) => c.execute(package_path, build_config),
        Command::Test(c) => {
            let unit_test_config = UnitTestingConfig {
                instruction_execution_bound: c.test.instruction_execution_bound,
                filter: c.test.filter.clone(),
                list: c.test.list,
                num_threads: c.test.num_threads,
                report_statistics: c.test.report_statistics,
                report_storage_on_error: c.test.report_storage_on_error,
                check_stackless_vm: c.test.check_stackless_vm,
                verbose: c.test.verbose_mode,

                ..UnitTestingConfig::default_with_bound(None)
            };
            let result = c.execute(package_path, build_config, unit_test_config)?;

            // Return a non-zero exit code if any test failed
            if let UnitTestResult::Failure = result {
                std::process::exit(1)
            }

            Ok(())
        }
        Command::New(c) => c.execute(package_path),
    }
}
