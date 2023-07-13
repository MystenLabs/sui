// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod utils;
use shared_crypto::intent::Intent;
use sui_keys::keystore::{AccountKeystore, InMemKeystore, Keystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{quorum_driver_types::ExecuteTransactionRequestType, transaction::Transaction},
    SuiClientBuilder,
};
use utils::request_tokens_from_faucet;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_testnet().await?; // testnet Sui network
    println!("Sui testnet version{:?}", sui.api_version());

    // Generate two Sui addresses and corresponding keys in memory
    let keystore = Keystore::InMem(InMemKeystore::new_insecure_for_tests(2));
    let addresses = keystore.addresses();
    let sender = addresses
        .get(0)
        .expect("Keystore did not manage to generate two keys in memory. Aborting");
    let recipient = addresses
        .get(1)
        .expect("Keystore did not manage to generate two keys in memory. Aborting");

    // Search for the coins in the sender's address
    let coins = sui
        .coin_read_api()
        .get_coins(*sender, None, None, None)
        .await?;

    println!("Address {sender} has {} coins", coins.data.len());
    println!();
    println!();
    if coins.next_cursor.is_none() {
        // Add coins to the newly created sender_address
        request_tokens_from_faucet(*sender).await?;
    }
    let Some(coin) = sui
        .coin_read_api()
        .get_coins(*sender, None, None, None)
        .await?.next_cursor else {panic!("Faucet did not work correctly and the provided Sui address has no coins")};

    // Programmable transactions allows the user to bundle a number of actions into one transaction
    let txb = sui.transaction_builder();

    // Split coin
    let txb_res = txb
        .split_coin(*sender, coin, vec![1], None, 5000000)
        .await?;
    println!("Split coin transaction data: {:?}", txb_res);
    println!();
    println!();

    // Transfer object
    let txb_res = txb
        .transfer_object(*sender, coin, None, 5000000, *recipient)
        .await?;
    println!("Tranfer object data: {:?}", txb_res);
    println!();
    println!();

    // Sign transaction
    let signature = keystore.sign_secure(sender, &txb_res, Intent::sui_transaction())?;

    // Execute the transaction
    print!("Executing the transaction...");
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(txb_res, Intent::sui_transaction(), vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    print!("done\n Transaction information: ");
    println!("{:?}", transaction_response);

    let coins = sui
        .coin_read_api()
        .get_coins(*recipient, None, None, None)
        .await?;

    println!(
        "After the transfer, the recipient address {recipient} has {} coins",
        coins.data.len()
    );
    Ok(())
}
