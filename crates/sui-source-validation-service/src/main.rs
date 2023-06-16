// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(msim))]
mod inner {

    use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
    use sui_sdk::wallet_context::WalletContext;

    use sui_source_validation_service::{initialize, serve};

    pub async fn main() -> anyhow::Result<()> {
        let config = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
        let context = WalletContext::new(&config, None, None).await?;
        let package_paths = vec![];
        initialize(&context, package_paths).await?;
        serve()?.await.map_err(anyhow::Error::from)
    }
}

#[cfg(msim)]
mod inner {
    pub async fn main() -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    inner::main().await
}
