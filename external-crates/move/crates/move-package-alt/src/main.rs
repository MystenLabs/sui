// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};
use move_package_alt::cli::{Build, Parse};

#[derive(Debug, Parser, Clone)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Build(Build),
    /// Compile the package
    Compile,
    /// Run tests for the package
    Test,
    /// Parse a manifest or lockfile, or both
    Parse(Parse),
}

impl Commands {
    pub fn execute(&self) {
        match self {
            Commands::Build(b) => b.execute(),
            Commands::Compile => {
                println!("Compiling package");
            }
            Commands::Test => {
                println!("Running tests for package");
            }
            Commands::Parse(p) => p.execute(),
        }
    }
}

impl Cli {
    pub fn execute(&self) {
        self.command.execute();
    }
}

fn main() {
    let cli = Cli::parse();
    cli.execute();
}
