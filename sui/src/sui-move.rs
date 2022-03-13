// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use colored::Colorize;
use std::path::Path;
use structopt::clap::App;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum MoveCommands {
    /// Build and verify Move project
    #[structopt(name = "build")]
    Build,

    /// Run all Move unit tests
    #[structopt(name = "test")]
    Test,
}

impl MoveCommands {
    pub fn execute(&self, path: &Path, is_std_framework: bool) -> Result<(), anyhow::Error> {
        match self {
            Self::Build => {
                Self::build(path, is_std_framework)?;
                println!("{}", "Build Successful".bold().green());
                println!("Artifacts path: {:?}", path.join("build"));
            }
            Self::Test => {
                Self::build(path, is_std_framework)?;
                sui_framework::run_move_unit_tests(path)?;
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

#[derive(StructOpt)]
#[structopt(
    name = "Sui Move Development Tool",
    about = "Tool to build and test Move applications",
    rename_all = "kebab-case"
)]
struct MoveOpt {
    /// Path to the Move project root.
    #[structopt(long, default_value = "./")]
    path: String,
    /// Whether we are building/testing the std/framework code.
    #[structopt(long)]
    std: bool,
    /// Subcommands.
    #[structopt(subcommand)]
    cmd: MoveCommands,
}

fn main() -> Result<(), anyhow::Error> {
    let app: App = MoveOpt::clap();
    let options = MoveOpt::from_clap(&app.get_matches());
    let path = options.path;
    options.cmd.execute(path.as_ref(), options.std)
}
