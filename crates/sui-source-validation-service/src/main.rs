// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::Parser;

use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::wallet_context::WalletContext;

use sui_source_validation_service::{initialize, parse_config, serve};

#[derive(Parser, Debug)]
struct Args {
    config_path: PathBuf,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let package_config = parse_config(args.config_path)?;
    let sui_config = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let context = WalletContext::new(&sui_config, None, None).await?;
    initialize(&context, &package_config).await?;
    serve()?.await.map_err(anyhow::Error::from)
}
