// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::{
    types::{
        base_types::{ObjectID, SuiAddress},
        messages::Transaction,
    },
    SuiClientBuilder,
};
use sui_types::intent::Intent;
use sui_types::messages::ExecuteTransactionRequestType;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("https://fullnode.devnet.sui.io:443")
        .await?;
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };

    let my_address = SuiAddress::from_str("0x47722589dc23d63e82862f7814070002ffaaa465")?;
    let gas_object_id = ObjectID::from_str("0x273b2a83f1af1fda3ddbc02ad31367fcb146a814")?;
    let recipient = SuiAddress::from_str("0xbd42a850e81ebb8f80283266951d4f4f5722e301")?;

    // Create a sui transfer transaction
    let transfer_tx = sui
        .transaction_builder()
        .transfer_sui(my_address, gas_object_id, 1000, recipient, Some(1000))
        .await?;

    // Sign transaction
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let signature = keystore.sign_secure(&my_address, &transfer_tx, Intent::default())?;

    // Execute the transaction
    let transaction_response = sui
        .quorum_driver()
        .execute_transaction(
            Transaction::from_data(transfer_tx, Intent::default(), signature).verify()?,
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    println!("{:?}", transaction_response);

    Ok(())
}
