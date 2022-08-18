// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_sdk::{
    crypto::KeystoreType,
    types::{
        base_types::{ObjectID, SuiAddress},
        messages::Transaction,
    },
    SuiClient,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new_http_client("https://gateway.devnet.sui.io:443")?;
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
        .transfer_sui(my_address, gas_object_id, 1000, recipient, Some(1000))
        .await?;

    // Get signer from keystore
    let keystore = KeystoreType::File(keystore_path).init()?;
    let signer = keystore.signer(my_address);

    // Sign the transaction
    let transaction = Transaction::from_data(transfer_tx, &signer);

    // Execute the transaction
    let transaction_response = sui.execute_transaction(transaction).await?;

    println!("{:?}", transaction_response);

    Ok(())
}
