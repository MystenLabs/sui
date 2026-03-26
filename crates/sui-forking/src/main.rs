// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_forking::start_server;
use sui_forking::GraphQLQueryClient;
use sui_forking::Network;
use sui_forking::ServiceStore;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let network = Network::Mainnet;
    let client = GraphQLQueryClient::new(network.gql_endpoint())?;

    start_server(
        &client,
        None,
        |seq| ServiceStore::new(seq),
        "127.0.0.1",
        9001,
        None,
        None,
    )
    .await
}
