// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};
use move_package_alt::{
    cli::{Build, New, UpdateDeps},
    errors::PackageResult,
};

#[derive(Debug, Parser, Clone)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Build(Build),
    New(New),
    /// Run tests for the package
    Test,
    /// Repin the dependencies for an environment and update the lockfile
    UpdateDeps(UpdateDeps),
}

impl Commands {
    pub async fn execute(&self) -> PackageResult<()> {
        match self {
            Commands::Build(b) => b.execute().await,
            Commands::New(n) => n.execute(),
            Commands::Test => todo!(),
            Commands::UpdateDeps(u) => u.execute().await,
        }
    }
}

impl Cli {
    pub async fn execute(&self) -> PackageResult<()> {
        self.command.execute().await
    }
}

#[tokio::main]
async fn main() -> PackageResult<()> {
    let cli = Cli::parse();
    let result = cli.execute().await;
    if let Err(ref e) = result {
        e.emit();
    }
    result
}
