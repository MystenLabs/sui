// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_analyzer::analyzer;
use sui::package_hooks::SuiPackageHooks;
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::wallet_context::WalletContext;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct App {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let context = WalletContext::new(&config, None, None)?;
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    rt.block_on(async {
        SuiPackageHooks::register_from_ctx(&context).await.unwrap();
    });

    App::parse();
    analyzer::run();

    Ok(())
}
