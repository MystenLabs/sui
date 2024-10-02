// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;
use shared_crypto::intent::Intent;
use sui_config::{sui_config_dir, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        quorum_driver_types::ExecuteTransactionRequestType,
        transaction::{Argument, Command, Transaction, TransactionData},
    },
};
use utils::setup_for_write;

// This example shows how to use programmable transactions to chain multiple
// actions into one transaction. Specifically, the example retrieves two addresses
// from the local wallet, and then
// 1) finds a coin from the active address that has Sui,
// 2) splits the coin into one coin of 1000 MIST and the rest,
// 3  transfers the split coin to second Sui address,
// 4) signs the transaction,
// 5) executes it.
// For some of these actions it prints some output.
// Finally, at the end of the program it prints the number of coins for the
// Sui address that received the coin.
// If you run this program several times, you should see the number of coins
// for the recipient address increases.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1) get the Sui client, the sender and recipient that we will use
    // for the transaction, and find the coin we use as gas
    let (sui, sender, recipient) = setup_for_write().await?;

    // we need to find the coin we will use as gas
    let coins = sui
        .coin_read_api()
        .get_coins(sender, None, None, None)
        .await?;
    let coin = coins.data.into_iter().next().unwrap();

    // programmable transactions allows the user to bundle a number of actions into one transaction
    let mut ptb = ProgrammableTransactionBuilder::new();

    // 2) split coin
    // the amount we want in the new coin, 1000 MIST
    let split_coint_amount = ptb.pure(1000u64)?; // note that we need to specify the u64 type
    ptb.command(Command::SplitCoins(
        Argument::GasCoin,
        vec![split_coint_amount],
    ));

    // 3) transfer the new coin to a different address
    let argument_address = ptb.pure(recipient)?;
    ptb.command(Command::TransferObjects(
        vec![Argument::Result(0)],
        argument_address,
    ));

    // finish building the transaction block by calling finish on the ptb
    let builder = ptb.finish();

    let gas_budget = 5_000_000;
    let gas_price = sui.read_api().get_reference_gas_price().await?;
    // create the transaction data that will be sent to the network
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin.object_ref()],
        builder,
        gas_budget,
        gas_price,
    );

    // 4) sign transaction
    let keystore = FileBasedKeystore::new(&sui_config_dir()?.join(SUI_KEYSTORE_FILENAME))?;
    let signature = keystore.sign_secure(&sender, &tx_data, Intent::sui_transaction())?;

    // 5) execute the transaction
    print!("Executing the transaction...");
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    print!("done\n Transaction information: ");
    println!("{:?}", transaction_response);

    let coins = sui
        .coin_read_api()
        .get_coins(recipient, None, None, None)
        .await?;

    println!(
        "After the transfer, the recipient address {recipient} has {} coins",
        coins.data.len()
    );
    Ok(())
}
