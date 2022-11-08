// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::test::UnitTestResult;
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;

pub mod build;
pub mod coverage;
pub mod disassemble;
pub mod new;
pub mod prove;
pub mod unit_test;

#[derive(Parser)]
pub enum Command {
    Build(build::Build),
    Coverage(coverage::Coverage),
    Disassemble(disassemble::Disassemble),
    New(new::New),
    Prove(prove::Prove),
    Test(unit_test::Test),
    CalibrateCosts(Calib),
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
    build_config: BuildConfig,
    command: Command,
) -> anyhow::Result<()> {
    match command {
        Command::Build(c) => c.execute(package_path, build_config),
        Command::Coverage(c) => c.execute(package_path, build_config),
        Command::Disassemble(c) => c.execute(package_path, build_config),
        Command::New(c) => c.execute(package_path),
        Command::Prove(c) => c.execute(package_path, build_config),
        Command::Test(c) => {
            let unit_test_config = UnitTestingConfig {
                gas_limit: c.test.gas_limit,
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
        Command::CalibrateCosts(c) => {
            sui_framework::cost_calib::run_calibration(c.runs, c.summarize);
            Ok(())
        }
    }
}
