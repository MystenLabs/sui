// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use colored::Colorize;
use move_unit_test::UnitTestingConfig;
use std::path::Path;

#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum MoveCommands {
    /// Build and verify Move project
    #[clap(name = "build")]
    Build {
        /// Whether we are printing in base64.
        #[clap(long)]
        dump_bytecode_as_base64: bool,
    },

    /// Run all Move unit tests
    #[clap(name = "test")]
    Test(UnitTestingConfig),
}

impl MoveCommands {
    pub fn execute(&self, path: &Path, is_std_framework: bool) -> Result<(), anyhow::Error> {
        match self {
            Self::Build {
                dump_bytecode_as_base64,
            } => {
                if *dump_bytecode_as_base64 {
                    let compiled_modules =
                        sui_framework::build_move_package_to_base64(path, is_std_framework)?;
                    println!("{:?}", compiled_modules);
                } else {
                    Self::build(path, is_std_framework)?;
                    println!("Artifacts path: {:?}", path.join("build"));
                }
                println!("{}", "Build Successful".bold().green());
            }
            Self::Test(config) => {
                Self::build(path, is_std_framework)?;
                sui_framework::run_move_unit_tests(path, Some(config.clone()))?;
            }
        }
        Ok(())
    }

    fn build(path: &Path, is_std_framework: bool) -> Result<(), anyhow::Error> {
        if is_std_framework {
            sui_framework::get_sui_framework_modules(path)?;
        } else {
            sui_framework::build_and_verify_user_package(path)?;
        }
        Ok(())
    }
}
