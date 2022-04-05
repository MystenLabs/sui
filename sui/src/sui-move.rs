// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use colored::Colorize;
use move_unit_test::UnitTestingConfig;
use std::path::Path;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum MoveCommands {
    /// Build and verify Move project
    #[clap(name = "build")]
    Build {
        /// Whether we are printing in hex.
        #[clap(long)]
        dump_bytecode_as_hex: bool,
    },

    /// Run all Move unit tests
    #[clap(name = "test")]
    Test(UnitTestingConfig),
}

impl MoveCommands {
    pub fn execute(&self, path: &Path, is_std_framework: bool) -> Result<(), anyhow::Error> {
        match self {
            Self::Build {
                dump_bytecode_as_hex,
            } => {
                if *dump_bytecode_as_hex {
                    let compiled_modules =
                        sui_framework::build_move_package_to_hex(path, is_std_framework)?;
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

#[derive(Parser)]
#[clap(
    name = "Sui Move Development Tool",
    about = "Tool to build and test Move applications",
    rename_all = "kebab-case"
)]
struct MoveOpt {
    /// Path to the Move project root.
    #[clap(long, default_value = "./")]
    path: String,
    /// Whether we are building/testing the std/framework code.
    #[clap(long)]
    std: bool,
    /// Subcommands.
    #[clap(subcommand)]
    cmd: MoveCommands,
}

fn main() -> Result<(), anyhow::Error> {
    let options = MoveOpt::parse();
    let path = options.path;
    options.cmd.execute(path.as_ref(), options.std)
}
