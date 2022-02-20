// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    // TODO: Add dev_mode as configurable option
}

impl MoveCommands {
    pub fn execute(&self, path: &Path) -> Result<(), anyhow::Error> {
        match self {
            Self::Build => {
                sui_framework::build_and_verify_user_package(path, false)?;
            }
            Self::Test => {
                sui_framework::build_and_verify_user_package(path, true).unwrap();
                sui_framework::run_move_unit_tests(path)?;
            }
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
    /// Subcommands.
    #[structopt(subcommand)]
    cmd: MoveCommands,
}

fn main() -> Result<(), anyhow::Error> {
    let app: App = MoveOpt::clap();
    let options = MoveOpt::from_clap(&app.get_matches());
    let path = options.path;
    options.cmd.execute(path.as_ref())
}
