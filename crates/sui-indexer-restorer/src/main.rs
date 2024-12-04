// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use tracing::info;

use sui_indexer_restorer::restore;
use sui_indexer_restorer::Args;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    info!("Starting indexer restorer from epoch {}", args.start_epoch);
    restore(&args).await?;
    info!("Finished indexer restorer!");
    Ok(())
}
