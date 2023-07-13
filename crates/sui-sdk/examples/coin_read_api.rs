// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;

use futures::{future, stream::StreamExt};
use sui_sdk::SuiClientBuilder;
use utils::sui_address_for_examples;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_testnet().await?; // testnet Sui network
    println!("Sui testnet version{:?}", sui.api_version());
    // create a random Sui address for examples.
    // Check utils module if you want to use a local wallet, or use SuiAddress::from_str("sui_address") for a specific address
    let active_address = sui_address_for_examples().await?;

    // ************ COIN READ API ************ //

    // Get coins
    let coins = sui
        .coin_read_api()
        .get_coins(active_address, None, None, Some(5)) // get the first five coins. Note that this can be filtered by coin type: coin_type: Some("0x2::sui::SUI".to_string())
        .await?;
    println!(" *** Coins ***");
    println!("{:?}", coins);
    println!(" *** Coins ***\n");

    // Get all coins
    let all_coins = sui
        .coin_read_api()
        .get_all_coins(active_address, None, Some(5)) // get the first five coins
        .await?;
    println!(" *** All coins ***");
    println!("{:?}", all_coins);
    println!(" *** All coins ***\n");

    // Get coins as a stream
    let coins_stream = sui.coin_read_api().get_coins_stream(active_address, None);

    println!(" *** Coins Stream ***");
    coins_stream
        .for_each(|coin| {
            println!("{:?}", coin);
            future::ready(())
        })
        .await;
    println!(" *** Coins Stream ***\n");

    // Select coins
    let select_coins = sui
        .coin_read_api()
        .select_coins(active_address, Some("0x2::sui::SUI".to_string()), 1, vec![])
        .await?;

    println!(" *** Select Coins ***");
    println!("{:?}", select_coins);
    println!(" *** Select Coins ***\n");

    // Balance
    let balance = sui
        .coin_read_api()
        .get_balance(active_address, None)
        .await?;
    // Total balance
    let total_balance = sui.coin_read_api().get_all_balances(active_address).await?;
    println!(" *** Balance + Total Balance *** ");
    println!("Balance: {:?}", balance);
    println!("Total Balance: {:?}", total_balance);
    println!(" *** Balance + Total Balance ***\n ");

    // Coin Metadata
    let coin_metadata = sui
        .coin_read_api()
        .get_coin_metadata("0x2::sui::SUI".to_string())
        .await?;

    println!(" *** Coin Metadata *** ");
    println!("{:?}", coin_metadata);
    println!(" *** Coin Metadata ***\n ");

    // Total Supply
    let total_supply = sui
        .coin_read_api()
        .get_total_supply("0x2::sui::SUI".to_string())
        .await?;
    println!(" *** Total Supply *** ");
    println!("{:?}", total_supply);
    println!(" *** Total Supply ***\n ");

    // ************ END OF COIN READ API ************ //
    Ok(())
}
