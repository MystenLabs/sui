// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_config::NodeConfig;
use sui_core::authority_server::AuthorityServerHandle;
use tracing::info;

pub struct SuiNode {
    authority_server: AuthorityServerHandle,
}

impl SuiNode {
    pub async fn start(config: &NodeConfig) -> Result<()> {
        let server = sui_core::make::make_server(config).await?.spawn().await?;

        info!(node =? config.public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        let node = SuiNode {
            authority_server: server,
        };

        node.authority_server.join().await?;

        Ok(())
    }
}
