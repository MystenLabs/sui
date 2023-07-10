// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use shared_crypto::intent::Intent;
use sui_keys::keystore::{AccountKeystore, InMemKeystore, Keystore};
use sui_sdk::rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_sdk::{types::transaction::Transaction, SuiClientBuilder};
mod utils;
use utils::request_tokens_from_faucet;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_localnet().await?;

    // Generate two Sui addresses and corresponding keys in memory
    let keystore = Keystore::InMem(InMemKeystore::new_insecure_for_tests(2));
    let addresses = keystore.addresses();
    let Some(sender_address) = addresses.get(0) else {panic!("Keystore did not manage to generate two keys in memory. Aborting")};
    let Some(recipient_address) = addresses.get(1) else {panic!("Keystore did not manage to generate two keys in memory. Aborting")};

    // Add coins to the newly created sender_address
    request_tokens_from_faucet(*sender_address).await?;

    // Search for the coins in the sender's address
    let coins = sui
        .coin_read_api()
        .get_coins(*sender_address, None, None, None)
        .await?;

    let Some(coin) = coins.next_cursor else {panic!("Faucet did not work correctly and the provided Sui address has no coins")};

    // Create a sui transfer transaction
    let transfer_tx = sui
        .transaction_builder()
        .transfer_sui(*sender_address, coin, 5000, *recipient_address, Some(1000))
        .await?;

    // Sign transaction
    let signature =
        keystore.sign_secure(sender_address, &transfer_tx, Intent::sui_transaction())?;

    // Execute the transaction
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(transfer_tx, Intent::sui_transaction(), vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    println!("{:?}", transaction_response);

    Ok(())
}
