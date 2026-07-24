// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use colored::Colorize;
use sui::sui_commands::SuiCommand;
use sui_types::exit_main;
use tracing::debug;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case",
    author,
    version = VERSION,
    propagate_version = true,
)]
struct Args {
    #[clap(subcommand)]
    command: SuiCommand,

    /// Display less output
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let args = Args::parse();
    let mut builder = telemetry_subscribers::TelemetryConfig::new()
        .with_log_level("error")
        .with_env();

    if !args.quiet {
        builder = builder.with_user_info_target("move_package_alt");
    }

    let _guard = builder.init();
    debug!("Sui CLI version: {VERSION}");
    // Lint diagnostics are rendered by every compile path (`sui client publish`/`upgrade`,
    // `sui move build`, ...), not just `sui move lint` — set the `--explain` hint command for the
    // whole binary so no path falls back to the default `move lint`.
    move_compiler::diagnostics::set_explain_command("sui move lint");
    exit_main!(args.command.execute().await);
}
