// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_fork::cli::Cli;

bin_version::bin_version!();

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    Cli::parse_with_version(VERSION).execute(VERSION).await
}
