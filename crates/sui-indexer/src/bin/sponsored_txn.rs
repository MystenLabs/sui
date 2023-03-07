// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use std::str::FromStr;
use sui_indexer::new_rpc_client;
use sui_json_rpc_types::GaslessTransactionBytes;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    messages::GaslessTransactionData,
};

/// Some hard coded example usage of the sponsored transaction gas station provided by shinami.

#[tokio::main]
async fn main() -> Result<()> {
    let test_config = TestConfig::parse();
    let fn_rpc_client = new_rpc_client(&test_config.fn_rpc_client_url).await?;
    let addy = AccountAddress::from_str("0x9131a6b39e93d5bc9379594068bf1dc62eefd4b9")?;
    let signer = SuiAddress::from(addy);

    let object_id = ObjectID::from_str("0xf7be95a22a85abc6e83df8b447aab4cef10cf805")?;

    let recipient_account = AccountAddress::from_str("0xd4caf026aa45790b4f7abe81ba181eb18c2c4c09")?;
    let recipient = SuiAddress::from(recipient_account);
    println!("constructing txn");

    let transaction = fn_rpc_client
        .transaction_builder()
        .transfer_object(signer, object_id, None, 5000, recipient)
        .await?;

    let gas_station_url = test_config.gas_station_url;

    println!("converting data to bytes");
    let gasless_data = GaslessTransactionData::from_transaction_data(transaction);
    let gasless_txn_bytes = GaslessTransactionBytes::from_data(gasless_data)?;

    let sponsored_bytes = fn_rpc_client
        .transaction_builder()
        .send_bytes_to_sponsor(gas_station_url.clone(), gasless_txn_bytes, 5000)
        .await?;

    println!("bytes_response {:?}", sponsored_bytes);

    let txn_digest = sponsored_bytes.result.unwrap().tx_digest;
    let status = fn_rpc_client
        .transaction_builder()
        .get_sponsored_transaction_status(gas_station_url.clone(), txn_digest.base58_encode())
        .await?;

    println!("status {:?}", status);

    Ok(())
}

#[derive(Parser)]
#[clap(name = "Sponsored Transaction Script")]
pub struct TestConfig {
    #[clap(long)]
    pub fn_rpc_client_url: String,
    #[clap(long)]
    pub gas_station_url: String,
}
