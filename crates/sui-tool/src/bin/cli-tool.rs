// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use clap::*;
use colored::Colorize;
use sui_types::exit_main;

use sui_tool::commands::ToolCommand;

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let bin_name = env!("CARGO_BIN_NAME");
    let cmd: ToolCommand = ToolCommand::parse();
    let _guard = telemetry_subscribers::TelemetryConfig::new(bin_name)
        .with_env()
        .init();

    exit_main!(cmd.execute().await);
}
