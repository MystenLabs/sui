// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};
use move_package_alt::{
    cli::{Build, Graph, New, Parse},
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
    Graph(Graph),
    New(New),
    /// Run tests for the package
    Test,
    /// Parse a manifest or lockfile, or both
    Parse(Parse),
}

impl Commands {
    pub async fn execute(&self) -> PackageResult<()> {
        match self {
            Commands::Build(b) => b.execute().await,
            Commands::Graph(g) => g.execute().await,
            Commands::New(n) => n.execute(),
            Commands::Test => todo!(),
            Commands::Parse(p) => p.execute(),
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
    cli.execute().await
}
