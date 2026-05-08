// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;

use sui_fork::cli::Cli;

bin_version::bin_version!();

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    Cli::parse().execute(VERSION).await
}
