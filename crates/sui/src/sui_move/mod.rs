// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_cli::package::cli::{PackageCommand, UnitTestResult};
use move_package::BuildConfig;
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;

pub mod build;
pub mod unit_test;

pub fn execute_move_command(
    package_path: PathBuf,
    dump_bytecode_as_base64: bool,
    build_config: BuildConfig,
    command: PackageCommand,
) -> anyhow::Result<()> {
    match command {
        PackageCommand::Build => {
            build::execute(&package_path, dump_bytecode_as_base64, build_config)
        }
        PackageCommand::UnitTest {
            instruction_execution_bound,
            filter,
            list,
            num_threads,
            report_statistics,
            report_storage_on_error,
            check_stackless_vm,
            verbose_mode,
            compute_coverage,
        } => {
            let unit_test_config = UnitTestingConfig {
                instruction_execution_bound,
                filter,
                list,
                num_threads,
                report_statistics,
                report_storage_on_error,
                check_stackless_vm,
                verbose: verbose_mode,

                ..UnitTestingConfig::default_with_bound(None)
            };
            let result = unit_test::execute(
                &package_path,
                dump_bytecode_as_base64,
                build_config,
                unit_test_config,
                compute_coverage,
            )?;

            // Return a non-zero exit code if any test failed
            if let UnitTestResult::Failure = result {
                std::process::exit(1)
            }

            Ok(())
        }
        PackageCommand::New { .. } => unimplemented!("'new' command not yet supported"),
        PackageCommand::Info => unimplemented!("'info' command not yet supported"),
        PackageCommand::ErrMapGen { .. } => unimplemented!("'errmap' command not yet supported"),
        PackageCommand::Prove { .. } => unimplemented!("'prove' command not yet supported"),
        PackageCommand::CoverageReport { .. } => {
            unimplemented!("'coverage' command not yet supported")
        }
        PackageCommand::BytecodeView { .. } => {
            unimplemented!("'disassemble' command not yet supported")
        }
    }
}
