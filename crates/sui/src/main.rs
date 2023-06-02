// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use colored::Colorize;
use std::env;
use sui::sui_commands::SuiCommand;
use sui_types::exit_main;
use tracing::debug;

const SUI_CLI_LOG_FILE_ENABLE: &str = "SUI_CLI_LOG_FILE_ENABLE";

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        let version = git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = ""
        );

        if version.is_empty() {
            panic!("unable to query git revision");
        }
        version
    }
};
const VERSION: &str = const_str::concat!(env!("CARGO_PKG_VERSION"), "-", GIT_REVISION);

pub fn read_log_file_flag_env() -> Option<u8> {
    env::var(SUI_CLI_LOG_FILE_ENABLE).ok()?.parse::<u8>().ok()
}

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct Args {
    #[clap(subcommand)]
    command: SuiCommand,
}

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let bin_name = env!("CARGO_BIN_NAME");
    let args = Args::parse();
    let _guard = match args.command {
        SuiCommand::Console { .. } | SuiCommand::Client { .. } => {
            let mut t = telemetry_subscribers::TelemetryConfig::new().with_env();
            if let Some(flag) = read_log_file_flag_env() {
                if flag > 0 {
                    t = t.with_log_file(&format!("{bin_name}.log"));
                }
            };
            t.init()
        }
        _ => telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init(),
    };

    debug!("Sui CLI version: {VERSION}");

    exit_main!(args.command.execute().await);
}
