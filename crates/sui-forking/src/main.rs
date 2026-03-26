// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_forking::Network;
use sui_forking::start_server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let host = "127.0.0.1";
    let server_port = 9001;

    start_server(Network::Mainnet, None, host, server_port, None, None).await?;

    Ok(())
}
