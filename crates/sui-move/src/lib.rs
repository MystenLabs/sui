// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
#[cfg(feature = "unit_test")]
use move_cli::base::test::UnitTestResult;
use move_package::BuildConfig;
#[cfg(feature = "unit_test")]
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;

#[cfg(feature = "build")]
pub mod build;
#[cfg(feature = "coverage")]
pub mod coverage;
#[cfg(feature = "disassemble")]
pub mod disassemble;
pub mod new;
#[cfg(feature = "prove")]
pub mod prove;
#[cfg(feature = "unit_test")]
pub mod unit_test;

#[derive(Parser)]
pub enum Command {
    #[cfg(feature = "build")]
    Build(build::Build),
    #[cfg(feature = "coverage")]
    Coverage(coverage::Coverage),
    #[cfg(feature = "disassemble")]
    Disassemble(disassemble::Disassemble),
    New(new::New),
    #[cfg(feature = "prove")]
    Prove(prove::Prove),
    #[cfg(feature = "unit_test")]
    Test(unit_test::Test),
}
#[derive(Parser)]
pub struct Calib {
    #[clap(name = "runs", short = 'r', long = "runs", default_value = "1")]
    runs: usize,
    #[clap(name = "summarize", short = 's', long = "summarize")]
    summarize: bool,
}

pub fn execute_move_command(
    package_path: Option<PathBuf>,
    #[allow(unused_variables)] build_config: BuildConfig,
    command: Command,
) -> anyhow::Result<()> {
    match command {
        #[cfg(feature = "build")]
        Command::Build(c) => c.execute(package_path, build_config),
        #[cfg(feature = "coverage")]
        Command::Coverage(c) => c.execute(package_path, build_config),
        #[cfg(feature = "disassemble")]
        Command::Disassemble(c) => c.execute(package_path, build_config),
        Command::New(c) => c.execute(package_path),
        #[cfg(feature = "prove")]
        Command::Prove(c) => c.execute(package_path, build_config),
        #[cfg(feature = "unit_test")]
        Command::Test(c) => {
            let unit_test_config = UnitTestingConfig {
                gas_limit: c.test.gas_limit,
                filter: c.test.filter.clone(),
                list: c.test.list,
                num_threads: c.test.num_threads,
                report_statistics: c.test.report_statistics.clone(),
                report_storage_on_error: c.test.report_storage_on_error,
                check_stackless_vm: c.test.check_stackless_vm,
                verbose: c.test.verbose_mode,
                ignore_compile_warnings: c.test.ignore_compile_warnings,
                ..UnitTestingConfig::default_with_bound(None)
            };
            let result = c.execute(package_path, build_config, unit_test_config)?;

            // Return a non-zero exit code if any test failed
            if let UnitTestResult::Failure = result {
                std::process::exit(1)
            }

            Ok(())
        }
    }
}
