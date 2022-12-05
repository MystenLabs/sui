// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::{
    types::{
        base_types::{ObjectID, SuiAddress},
        messages::Transaction,
    },
    SuiClient,
};
use sui_types::messages::ExecuteTransactionRequestType;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", None, None).await?;
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };

    let my_address =
        SuiAddress::from_str("sui1sau0w2w6j38k2tqtx0t87w9uaackz4gq5qagletswavsnc3n59ksjtk7gf")?;
    let gas_object_id =
        ObjectID::from_str("sui1qqqqqqqqqqqqqqqqqqqzwwe2s0c678768hduq2knzdnlev2x4q2q83tpuj")?;
    let recipient =
        SuiAddress::from_str("sui1lk3pxl3kutypw423eknhm2xq2sd83ugrsrw89g07hjwx7j9uvg3qntl4r0")?;

    // Create a sui transfer transaction
    let transfer_tx = sui
        .transaction_builder()
        .transfer_sui(my_address, gas_object_id, 1000, recipient, Some(1000))
        .await?;

    // Sign transaction
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let signature = keystore.sign(&my_address, &transfer_tx.to_bytes())?;

    // Execute the transaction
    let transaction_response = sui
        .quorum_driver()
        .execute_transaction(
            Transaction::from_data(transfer_tx, signature).verify()?,
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    println!("{:?}", transaction_response);

    Ok(())
}
