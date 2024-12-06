// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::*;
use colored::Colorize;
use move_package::BuildConfig as MoveBuildConfig;
use sui_move::execute_move_command;
use sui_types::exit_main;
use tracing::debug;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "Sui-Move CLI",
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct Args {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,
    /// If true, run the Move bytecode verifier on the bytecode from a successful build
    #[clap(long, global = true)]
    pub run_bytecode_verifier: bool,
    /// If true, print build diagnostics to stderr--no printing if false
    #[clap(long, global = true)]
    pub print_diags_to_stderr: bool,
    /// Package build options
    #[clap(flatten)]
    pub build_config: MoveBuildConfig,
    /// Subcommands.
    #[clap(subcommand)]
    pub cmd: sui_move::Command,
}

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let bin_name = env!("CARGO_BIN_NAME");
    let args = Args::parse();
    // let _guard = match args.command {
    //     SuiCommand::Console { .. } | SuiCommand::Client { .. } => {
    //         telemetry_subscribers::TelemetryConfig::new()
    //             .with_log_file(&format!("{bin_name}.log"))
    //             .with_env()
    //             .init()
    //     }
    //     _ => telemetry_subscribers::TelemetryConfig::new()
    //         .with_env()
    //         .init(),
    // };

    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_log_file(&format!("{bin_name}.log"))
        .with_env()
        .init();
    debug!("Sui-Move CLI version: {VERSION}");

    exit_main!(execute_move_command(
        args.package_path.as_deref(),
        args.build_config,
        args.cmd
    ));
}
