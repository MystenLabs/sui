mod utils;
use shared_crypto::intent::Intent;
use sui_config::{sui_config_dir, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        base_types::{ObjectID},
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        quorum_driver_types::ExecuteTransactionRequestType,
        transaction::{Argument, CallArg, Command, ProgrammableMoveCall, Transaction, TransactionData},
        Identifier,
    },
};
use utils::setup_for_write;
use bcs;
use move_core_types::parser::parse_type_tag; // Import the parse_type_tag function

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

    // Create an Argument::Input for Pure 5 value of type u64
    let tick_from_mid = 5u64;
    let input_argument = CallArg::Pure(bcs::to_bytes(&tick_from_mid).unwrap());
    ptb.input(input_argument)?;

    // Define the base_coin_type and quote_coin_type using parse_type_tag
    let base_coin_type = parse_type_tag("0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8::deep::DEEP")?;
    let quote_coin_type = parse_type_tag("0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI")?;

    // Add the Move call to the PTB
    let pkg_id = "0xc89b2bd6172c077aec6e8d7ba201e99c32f9770cdae7be6dac9d95132fff8e8e";
    let package = ObjectID::from_hex_literal(pkg_id)?;
    let module = Identifier::new("pool")?;
    let function = Identifier::new("get_level2_ticks_from_mid")?;
    let sui_clock_object_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000006"
    )?;

    ptb.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments: vec![base_coin_type, quote_coin_type],
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
