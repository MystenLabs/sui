---
title: Sui Rust SDK
---

## Overview
The Sui SDK is a collection of rust JSON-RPC wrapper and crypto utilities that you can use to interact with the Sui Gateway and Sui Full Node.
The `SuiClient` can be used to create a http(`SuiClient::new_http_client`) or a websocket client(`SuiClient::new_ws_client`).  
See [JSON-RPC doc](json-rpc.md#sui-json-rpc-methods) for list of available methods.

> Note: As of v0.6.0, the web socket client is for subscription only, please use http client for other api methods.

## Examples
Add the sui-sdk crate in your Cargo.toml:
```toml
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui" }
```
Use the devnet branch if you are connecting to the devnet. 
```toml
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui", branch = "devnet" }
```

### Example 1 - Get all objects owned by an address
```rust
use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new_http_client("https://gateway.devnet.sui.io:443")?;
    let address = SuiAddress::from_str("0xec11cad080d0496a53bafcea629fcbcfff2a9866")?;
    let objects = sui.get_objects_owned_by_address(address).await?;
    println!("{:?}", objects);
    Ok(())
}
```
This will print a list of object summaries owned by the address "0xec11cad080d0496a53bafcea629fcbcfff2a9866".
You can verify the result with the [Sui explorer](https://explorer.devnet.sui.io/) if you are using the Sui devnet.

### Example 2 - Create and execute transaction
```rust
use std::str::FromStr;
use sui_sdk::crypto::{Keystore, SuiKeystore};
use sui_sdk::types::base_types::{ObjectID, SuiAddress};
use sui_sdk::types::sui_serde::Base64;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new_http_client("https://gateway.devnet.sui.io:443")?;
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };
    let keystore = SuiKeystore::load_or_create(&keystore_path)?;

    let my_address = SuiAddress::from_str("0x47722589dc23d63e82862f7814070002ffaaa465")?;
    let gas_object_id = ObjectID::from_str("0x273b2a83f1af1fda3ddbc02ad31367fcb146a814")?;
    let recipient = SuiAddress::from_str("0xbd42a850e81ebb8f80283266951d4f4f5722e301")?;

    // Create a sui transfer transaction
    let transfer_tx = sui
        .transfer_sui(my_address, gas_object_id, 1000, recipient, Some(1000))
        .await?;

    // Sign the transaction
    let signature = keystore.sign(&my_address, &transfer_tx.tx_bytes.to_vec()?)?;

    // Execute the transaction
    let transaction_response = sui
        .execute_transaction(
            transfer_tx.tx_bytes,
            Base64::from_bytes(signature.signature_bytes()),
            Base64::from_bytes(signature.public_key_bytes()),
        )
        .await?;

    println!("{:?}", transaction_response);

    Ok(())
}
```

### Example 3 - Event subscription
```rust
use futures::StreamExt;
use sui_sdk::rpc_types::SuiEventFilter;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new_ws_client("ws://127.0.0.1:9001").await?;
    let mut subscribe_all = sui.subscribe_event(SuiEventFilter::All(vec![])).await?;
    loop {
        println!("{:?}", subscribe_all.next().await);
    }
}
```
> Note: You will need to connect to a fullnode for the Event subscription service, see [Fullnode setup](fullnode.md#fullnode-setup) if you want to run a fullnode.


## Larger Examples
[Tic Tac Toe](../../../crates/sui-sdk/README.md)