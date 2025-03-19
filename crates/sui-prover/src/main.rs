// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::*;
use colored::Colorize;
use sui_prover::{execute_command, Command};
use tracing::debug;

bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "A command-line tool for formal verification of Move code in Sui projects. When run in the root of a project, it executes all proofs automatically.",
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct Args {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,
    /// Subcommands.
    #[clap(subcommand)]
    pub cmd: Command,
}

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let bin_name = env!("CARGO_BIN_NAME");
    let args = Args::parse();

    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_log_file(&format!("{bin_name}.log"))
        .with_env()
        .init();

    debug!("Sui-Prover CLI version: {VERSION}");

    let result: Result<(), anyhow::Error> = execute_command(
        args.package_path.as_deref(),
        args.cmd
    );

    match result {
        Ok(_) => (),
        Err(err) => {
            let err = format!("{:?}", err);
            println!("{}", err.bold().red());
            std::process::exit(1);
        }
    }
}
