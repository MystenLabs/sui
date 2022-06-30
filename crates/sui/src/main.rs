// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use clap::*;
use colored::Colorize;
use sui::sui_commands::SuiCommand;
use sui_types::exit_main;
use tracing::debug;
#[cfg(test)]
#[path = "unit_tests/cli_tests.rs"]
mod cli_tests;

#[tokio::main]
async fn main() {
    let bin_name = env!("CARGO_BIN_NAME");
    let cmd: SuiCommand = SuiCommand::parse();
    let _guard = match cmd {
        SuiCommand::Console { .. } | SuiCommand::Client { .. } => {
            telemetry_subscribers::TelemetryConfig::new(bin_name)
                .with_log_file(&format!("{bin_name}.log"))
                .with_env()
                .init()
        }
        _ => telemetry_subscribers::TelemetryConfig::new(bin_name)
            .with_env()
            .init(),
    };

    if let Some(git_rev) = option_env!("GIT_REVISION") {
        debug!("Sui CLI built at git revision {git_rev}");
    }
    exit_main!(cmd.execute().await);
}
