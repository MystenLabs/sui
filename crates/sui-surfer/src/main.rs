// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use sui::wallet_commands::WalletContext;
use sui_config::{sui_config_dir, SUI_WALLET_CONFIG};
use sui_types::base_types::ObjectID;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    /// Path to wallet whose objects/keys the surfer will use
    #[clap(long)]
    wallet_config: Option<PathBuf>,

    /// Surf all `script` functions in these packages
    #[clap(long)]
    packages: Vec<ObjectID>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let wallet_conf_path = args
        .wallet_config
        .clone()
        .unwrap_or(sui_config_dir()?.join(SUI_WALLET_CONFIG));

    let mut wallet = WalletContext::new(&wallet_conf_path)?;
    let seed = [0x1; 32];
    let mut surfer = sui_surfer::surfer::SurferState::new(seed);

    for package in args.packages {
        surfer.surf_package(package, &mut wallet).await?
    }
    println!("{:?}", surfer.stats());

    Ok(())
}
