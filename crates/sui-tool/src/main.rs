// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use clap::*;
use colored::Colorize;
use sui_tool::commands::ToolCommand;
use sui_types::exit_main;

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let cmd: ToolCommand = ToolCommand::parse();
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    exit_main!(cmd.execute().await);
}
