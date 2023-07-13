// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;

use sui_sdk::SuiClientBuilder;
use utils::sui_address_for_examples;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_testnet().await?; // testnet Sui network
    println!("Sui testnet version{:?}", sui.api_version());
    // create a random Sui address for examples. Check utils module if you want to use a local wallet, or use SuiAddress::from_str("sui_address") for a specific address
    let active_address = sui_address_for_examples().await?;

    // ************ GOVERNANCE API ************ //

    // Stakes
    let stakes = sui.governance_api().get_stakes(active_address).await?;

    println!(" *** Stakes ***");
    println!("{:?}", stakes);
    println!(" *** Stakes ***\n");

    // Committee Info
    let committee = sui.governance_api().get_committee_info(None).await?; // None defaults to the last epoch

    println!(" *** Committee Info ***");
    println!("{:?}", committee);
    println!(" *** Committee Info ***\n");

    // Latest Sui System State
    let sui_system_state = sui.governance_api().get_latest_sui_system_state().await?;

    println!(" *** Sui System State ***");
    println!("{:?}", sui_system_state);
    println!(" *** Sui System State ***\n");

    // Reference Gas Price
    let reference_gas_price = sui.governance_api().get_reference_gas_price().await?;

    println!(" *** Reference Gas Price ***");
    println!("{:?}", reference_gas_price);
    println!(" *** Reference Gas Price ***\n");

    // ************ END OF GOVERNANCE API ************ //
    Ok(())
}
