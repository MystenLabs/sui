// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// use clap::{Parser, Subcommand};
use move_package_alt::errors::PackageResult;

// bin_version::bin_version!();
//
// #[derive(Debug, Parser, Clone)]
// #[command(version, about, long_about = None)]
// pub struct Cli {
//     #[clap(subcommand)]
//     command: Commands,
// }
//
// #[derive(Debug, Clone, Subcommand)]
// pub enum Commands {
//     Build(Build),
//     Publish(Publish),
//     Upgrade(Upgrade),
// }
//
// impl Commands {
//     pub async fn execute(&self) -> PackageResult<()> {
//         match self {
//             Commands::Build(b) => b.execute().await,
//             Commands::Publish(p) => p.execute(VERSION).await,
//             Commands::Upgrade(u) => u.execute(VERSION).await,
//         }
//     }
// }
//
// impl Cli {
//     pub async fn execute(&self) -> PackageResult<()> {
//         self.command.execute().await
//     }
// }

// #[tokio::main]
fn main() -> PackageResult<()> {
    Ok(())
    // let cli = Cli::parse();
    // cli.execute().await
}
