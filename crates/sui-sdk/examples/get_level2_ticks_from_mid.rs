use anyhow::anyhow;
use sui_json_rpc_types::{SuiObjectData, SuiObjectDataOptions, SuiObjectResponse};
use sui_sdk::SuiClientBuilder;

use std::str::FromStr;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    digests::ObjectDigest,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, TransactionKind},
    Identifier, TypeTag,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui_client = SuiClientBuilder::default().build_testnet().await?;
    let mut ptb = ProgrammableTransactionBuilder::new();

    let pool_address = ObjectID::from_hex_literal(
        "0x2decc59a6f05c5800e5c8a1135f9d133d1746f562bf56673e6e81ef4f7ccd3b7",
    )?;
    // get the latest pool object version
    let pool_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(pool_address, SuiObjectDataOptions::full_content())
        .await?;
    let pool_data: &SuiObjectData = pool_object
        .data
        .as_ref()
        .ok_or(anyhow!("Missing data in pool object response"))?;

    let pool_object_ref: ObjectRef = (
        pool_data.object_id.clone(),
        SequenceNumber::from(pool_data.version),
        ObjectDigest::from(pool_data.digest.clone()),
    );

    // mark pool_object_ref as the first input. Later used as Argument::Input(0)
    let pool_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(pool_object_ref));
    ptb.input(pool_input)?;

    // mark ticks_from_mid as the second input. Later used as Argument::Input(1)
    let ticks_from_mid = 10u64;
    let input_argument = CallArg::Pure(bcs::to_bytes(&ticks_from_mid).unwrap());
    ptb.input(input_argument)?;

    // Convert the sui_clock_object_id string to ObjectID
    let sui_clock_object_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000006",
    )?;
    // get the latest clock object version
    let sui_clock_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(sui_clock_object_id, SuiObjectDataOptions::full_content())
        .await?;
    let clock_data: &SuiObjectData = sui_clock_object
        .data
        .as_ref()
        .ok_or(anyhow!("Missing data in clock object response"))?;

    let sui_clock_object_ref: ObjectRef = (
        clock_data.object_id.clone(),
        SequenceNumber::from(clock_data.version),
        ObjectDigest::from(clock_data.digest.clone()),
    );

    // mark sui_clock_object_ref as the third input. Later used as Argument::Input(2)
    let clock_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(sui_clock_object_ref));
    ptb.input(clock_input)?;

    // Correctly use TypeTag for base_coin_type and quote_coin_type
    let base_coin_type: TypeTag = TypeTag::from_str(
        "0x36dbef866a1d62bf7328989a10fb2f07d769f4ee587c0de4a0a256e57e0a58a8::deep::DEEP",
    )?;
    let quote_coin_type: TypeTag = TypeTag::from_str(
        "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
    )?;

    // Add the Move call to the PTB
    let pkg_id = "0xc89b2bd6172c077aec6e8d7ba201e99c32f9770cdae7be6dac9d95132fff8e8e";
    let package = ObjectID::from_hex_literal(pkg_id).map_err(|e| anyhow!(e))?;
    let module = Identifier::new("pool").map_err(|e| anyhow!(e))?;
    let function = Identifier::new("get_level2_ticks_from_mid").map_err(|e| anyhow!(e))?;

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

    let builder = ptb.finish();
    let tx = TransactionKind::ProgrammableTransaction(builder);
    // use the read_api() to get the dev_inspect_transaction_block function.
    // this does not require you to input any gas coins.
    let result = sui_client
        .read_api()
        .dev_inspect_transaction_block(SuiAddress::default(), tx, None, None, None)
        .await?;

    // parse the results.
    let binding = result.results.unwrap();

    let bid_prices = &binding.get(0).unwrap().return_values.get(0).unwrap().0;
    let bid_parsed_prices: Vec<u64> = bcs::from_bytes(&bid_prices).unwrap();
    let bid_quantities = &binding.get(0).unwrap().return_values.get(1).unwrap().0;
    let bid_parsed_quantities: Vec<u64> = bcs::from_bytes(&bid_quantities).unwrap();

    let ask_prices = &binding.get(0).unwrap().return_values.get(2).unwrap().0;
    let ask_parsed_prices: Vec<u64> = bcs::from_bytes(&ask_prices).unwrap();
    let ask_quantities = &binding.get(0).unwrap().return_values.get(3).unwrap().0;
    let ask_parsed_quantities: Vec<u64> = bcs::from_bytes(&ask_quantities).unwrap();

    println!(
        "First {} bid ticks: {:?}",
        ticks_from_mid, bid_parsed_prices
    );
    println!(
        "First {} bid quantities: {:?}",
        ticks_from_mid, bid_parsed_quantities
    );
    println!(
        "First {} ask ticks: {:?}",
        ticks_from_mid, ask_parsed_prices
    );
    println!(
        "First {} ask quantities: {:?}",
        ticks_from_mid, ask_parsed_quantities
    );

    Ok(())
}
