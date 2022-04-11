// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use clap::*;
use sui::sui_commands::SuiCommand;

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
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "sui".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    };
    #[allow(unused)]
    let guard = telemetry_subscribers::init(config);

    let options: SuiOpt = SuiOpt::parse();
    options.command.execute().await
}
