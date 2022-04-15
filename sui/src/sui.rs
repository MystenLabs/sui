// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use clap::*;
use sui::sui_commands::SuiCommand;
use sui_utils::trace_utils;

#[cfg(test)]
#[path = "unit_tests/cli_tests.rs"]
mod cli_tests;

#[derive(Parser)]
#[clap(
    name = "Sui Local",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct SuiOpt {
    #[clap(subcommand)]
    command: SuiCommand,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let _guard = trace_utils::init_telemetry();

    let options: SuiOpt = SuiOpt::parse();
    options.command.execute().await
}
