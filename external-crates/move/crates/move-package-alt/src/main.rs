// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};
use move_package_alt::cli::{Build, New, UpdateDeps};

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
    pub async fn execute(&self) -> anyhow::Result<()> {
        match self {
            Commands::Build(b) => b.execute().await?,
            Commands::New(n) => n.execute()?,
            Commands::Test => todo!(),
            Commands::UpdateDeps(u) => u.execute().await?,
        };
        Ok(())
    }
}

impl Cli {
    pub async fn execute(&self) -> anyhow::Result<()> {
        self.command.execute().await
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.execute().await
}
