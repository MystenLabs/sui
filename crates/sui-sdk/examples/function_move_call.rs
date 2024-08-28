// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;
use anyhow::anyhow;
use shared_crypto::intent::Intent;
use sui_config::{sui_config_dir, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        base_types::ObjectID,
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        quorum_driver_types::ExecuteTransactionRequestType,
        transaction::{
            Argument, CallArg, Command, ProgrammableMoveCall, Transaction, TransactionData,
        },
        Identifier,
    },
};
use utils::setup_for_write;

// This example shows how to use programmable transactions to chain multiple
// commands into one transaction, and specifically how to call a function from a move package
// These are the following steps:
// 1) finds a coin from the active address that has Sui,
// 2) creates a PTB and adds an input to it,
// 3) adds a move call to the PTB,
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
    let (sui, sender, _recipient) = setup_for_write().await?;

    // Find a coin to use as gas
    let coins = sui.coin_read_api().get_coins(sender, None, None, None).await?;
    let coin = coins.data.into_iter().next().unwrap();

    // Create a programmable transaction builder (PTB)
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Define the pool address, baseCoin type, and quoteCoin type
    let pool_address = "0x2decc59a6f05c5800e5c8a1135f9d133d1746f562bf56673e6e81ef4f7ccd3b7";
    let pool_object_id = ObjectID::from_hex_literal(pool_address)?;
    let pool_input = CallArg::Object(bcs::to_bytes(&pool_address).unwrap());
    let pool_object_ref = sui.read_api().get_object(pool_object_id).await?.reference();
    let pool_input = CallArg::Object(pool_object_ref.into());
    ptb.input(pool_input)?;

    // Create an Argument::Input for Pure 5 value of type u64
    let tick_from_mid = 5u64;
    let input_argument = CallArg::Pure(bcs::to_bytes(&tick_from_mid).unwrap());

    // Add this input to the builder
    ptb.input(input_argument)?;

    let base_coin_type = "0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8::deep::DEEP";  // Example base coin type
    let quote_coin_type = "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";  // Example quote coin type

    // Add the Move call to the PTB
    let pkg_id = "0xc89b2bd6172c077aec6e8d7ba201e99c32f9770cdae7be6dac9d95132fff8e8e";
    let package = ObjectID::from_hex_literal(pkg_id).map_err(|e| anyhow!(e))?;
    let module = Identifier::new("pool").map_err(|e| anyhow!(e))?;
    let function = Identifier::new("get_level2_ticks_from_mid").map_err(|e| anyhow!(e))?;
    // Convert the sui_clock_object_id string to ObjectID
    let sui_clock_object_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000006"
    )?;

    // Fetch the object reference using the Sui API
    let sui_clock_object_ref = sui.read_api().get_object(sui_clock_object_id).await?.reference();

    // Use the full object reference to create CallArg::Object
    let clock_input = CallArg::Object(sui_clock_object_ref.into());
    ptb.input(clock_input)?;

    ptb.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments: vec![
            Identifier::new(base_coin_type).map_err(|e| anyhow!(e))?,
            Identifier::new(quote_coin_type).map_err(|e| anyhow!(e))?,
        ],
        arguments: vec![
            Argument::Input(0), // pool.address
            Argument::Input(1), // tickFromMid
            Argument::Input(2), // SUI_CLOCK_OBJECT_ID
        ],
    })));

    // Build the transaction block
    let builder = ptb.finish();

    let gas_budget = 10_000_000;
    let gas_price = sui.read_api().get_reference_gas_price().await?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin.object_ref()],
        builder,
        gas_budget,
        gas_price,
    );

    // Sign the transaction
    let keystore = FileBasedKeystore::new(&sui_config_dir()?.join(SUI_KEYSTORE_FILENAME))?;
    let signature = keystore.sign_secure(&sender, &tx_data, Intent::sui_transaction())?;

    // Execute the transaction
    print!("Executing the transaction...");
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    println!("{}", transaction_response);
    Ok(())
}
