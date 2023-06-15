// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::wallet_context::WalletContext;
use sui_source_validation_service::{initialize, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let context = WalletContext::new(&config, None, None).await?;
    let package_paths = vec![];
    initialize(&context, package_paths).await?;
    serve()?.await.map_err(anyhow::Error::from)
}
