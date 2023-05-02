---
title: Interact with Sui using the Rust SDK
---

## Overview

The [Sui SDK](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk) is a collection of Rust language JSON-RPC wrapper and crypto utilities you can use to interact with Sui.

Use the [`SuiClient`](cli-client.md) to create an HTTP or a WebSocket client (`SuiClient::new`). See the [JSON-RPC](json-rpc.md#sui-json-rpc-methods) documentation for the list of available methods.

**Note:** The WebSocket client supports only [subscription](event_api.md#subscribe-to-sui-events); use the HTTP client for other API methods.

## References

View the documentation for the [crates used in Sui](https://mystenlabs.github.io/sui/).

## Configuration

Add the `sui-sdk` crate in your [`Cargo.toml`](https://doc.rust-lang.org/cargo/reference/manifest.html) file:

```bash
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui" }
```

Include the `branch` argument to use a specific branch of the Sui repository:

```bash
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui", branch = "devnet" }
```

## Example 1 - Get all objects owned by an address

This code example prints a list of object summaries owned by the specified address.

```rust
use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::{SuiClient, SuiClientBuilder};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build(
      "https://fullnode.devnet.sui.io:443",
    ).await.unwrap();
    let address = SuiAddress::from_str("0xbcab7526033aa0e014f634bf51316715dda0907a7fab5a8d7e3bd44e634a4d44")?;
    let objects = sui.read_api().get_owned_objects(address).await?;
    println!("{:?}", objects.data);
    Ok(())
}
```

You can verify the result with the [Sui Explorer](https://suiexplorer.com/) if you are using a Sui Devnet Full node.

## Example 2 - Create and execute transaction

Use this example to conduct a transaction in Sui using the Sui Devnet Full node:

```rust
use std::str::FromStr;
use sui_sdk::{
    crypto::{FileBasedKeystore, Keystore},
    types::{
        base_types::{ObjectID, SuiAddress},
        crypto::Signature,
        messages::Transaction,
    },
    SuiClient,
    SuiClientBuilder,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build(
      "https://fullnode.devnet.sui.io:443",
    ).await.unwrap();
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };

    let my_address = SuiAddress::from_str("0xbcab7526033aa0e014f634bf51316715dda0907a7fab5a8d7e3bd44e634a4d44")?;
    let gas_object_id = ObjectID::from_str("0xe638c76768804cebc0ab43e103999886641b0269a46783f2b454e2f8880b5255")?;
    let recipient = SuiAddress::from_str("0x727b37454ab13d5c1dbb22e8741bff72b145d1e660f71b275c01f24e7860e5e5")?;

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
        .execute_transaction_block(Transaction::from_data(transfer_tx, Intent::default(), signature))

    println!("{:?}", transaction_response);

    Ok(())
}
```

## Example 3 - Event subscription

Use the WebSocket client to [subscribe to events](event_api.md#subscribe-to-sui-events).

```rust
use futures::StreamExt;
use sui_sdk::rpc_types::SuiEventFilter;
use sui_sdk::{SuiClient, SuiClientBuilder};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build(
      "https://fullnode.devnet.sui.io:443",
    ).await.unwrap();
    let mut subscribe_all = sui.event_api().subscribe_event(SuiEventFilter::All(vec![])).await?;
    loop {
        println!("{:?}", subscribe_all.next().await);
    }
}
```

**Note:** The Event subscription service requires a running Sui Full node. To learn more, see [Full node setup](fullnode.md#fullnode-setup).
