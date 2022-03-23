// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use structopt::StructOpt;
use sui::sui_commands::SuiCommand;

use tracing::info;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

#[cfg(test)]
#[path = "unit_tests/cli_tests.rs"]
mod cli_tests;

#[derive(StructOpt)]
#[structopt(
    name = "Sui Local",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct SuiOpt {
    #[structopt(subcommand)]
    command: SuiCommand,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // See [[dev-docs/observability.md]] for more information on span logging.
    if std::env::var("SUI_JSON_SPAN_LOGS").is_ok() {
        // Code to add logging/tracing config from environment, including RUST_LOG
        // See https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/#5-7-tracing-bunyan-formatter
        // Also Bunyan layer addes JSON logging for tracing spans with duration information
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let formatting_layer = BunyanFormattingLayer::new(
            "sui".into(),
            // Output the formatted spans to stdout.
            std::io::stdout,
        );
        // The `with` method is provided by `SubscriberExt`, an extension
        // trait for `Subscriber` exposed by `tracing_subscriber`
        let subscriber = Registry::default()
            .with(env_filter)
            .with(JsonStorageLayer)
            .with(formatting_layer);
        // `set_global_default` can be used by applications to specify
        // what subscriber should be used to process spans.
        set_global_default(subscriber).expect("Failed to set subscriber");

        info!("Enabling JSON and span logging");
    } else {
        // Initializes default logging.  Will use RUST_LOG but no JSON/span info.
        tracing_subscriber::fmt::init();
        info!("Standard user-friendly logging, no spans no JSON");
    }

    let options: SuiOpt = SuiOpt::from_args();
    options.command.execute().await
}
