// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk::SuiClientBuilder;

// This example shows the few basic ways to connect to a Sui network.
// There are several in-built methods for connecting to the
// Sui devnet, tesnet, and localnet (running locally),
// as well as a custom way for connecting to custom URLs.
// The example prints out the API versions of the different networks,
// and finally, it prints the list of available RPC methods
// and the list of subscriptions.
// Note that running this code will fail if there is no Sui network
// running locally on the default address: 127.0.0.1:9000

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("http://127.0.0.1:9000") // local network address
        .await?;
    println!("Sui local network version: {}", sui.api_version());

    // local Sui network, like the above one but using the dedicated function
    let sui_local = SuiClientBuilder::default().build_localnet().await?;
    println!("Sui local network version: {}", sui_local.api_version());

    // Sui devnet -- https://fullnode.devnet.sui.io:443
    let sui_devnet = SuiClientBuilder::default().build_devnet().await?;
    println!("Sui devnet version: {}", sui_devnet.api_version());

    // Sui testnet -- https://fullnode.testnet.sui.io:443
    let sui_testnet = SuiClientBuilder::default().build_testnet().await?;
    println!("Sui testnet version: {}", sui_testnet.api_version());

    // Sui mainnet -- https://fullnode.mainnet.sui.io:443
    let sui_mainnet = SuiClientBuilder::default().build_mainnet().await?;
    println!("Sui mainnet version: {}", sui_mainnet.api_version());

    println!("rpc methods: {:?}", sui_testnet.available_rpc_methods());
    println!(
        "available subscriptions: {:?}",
        sui_testnet.available_subscriptions()
    );

    Ok(())
}
