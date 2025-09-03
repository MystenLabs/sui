// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use tokio_util::sync::CancellationToken;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct FullNodeArgs {
    /// gRPC URL for full node operations such as executeTransaction and simulateTransaction.
    #[clap(long)]
    pub full_node_rpc_url: Option<String>,
}

/// A reader backed by the full node gRPC service.
#[derive(Clone)]
pub struct FullNodeClient {
    #[allow(dead_code)]
    client: Option<sui_rpc_api::client::Client>,
    #[allow(dead_code)]
    cancel: CancellationToken,
}

impl FullNodeClient {
    pub async fn new(args: FullNodeArgs, cancel: CancellationToken) -> anyhow::Result<Self> {
        let client = if let Some(url) = &args.full_node_rpc_url {
            Some(sui_rpc_api::client::Client::new(url).context("Failed to create gRPC client")?)
        } else {
            None
        };

        Ok(Self { client, cancel })
    }
}
