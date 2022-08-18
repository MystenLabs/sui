---
title: Interact with Sui over Rust SDK
---

## Overview
The [Sui SDK](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk) is a collection of Rust language JSON-RPC wrapper and crypto utilities you can use to interact with the [Sui Devnet Gateway](../build/devnet.md) and [Sui Full Node](fullnode.md).

The [`SuiClient`](cli-client.md) can be used to create an HTTP (`SuiClient::new_http_client`) or a WebSocket client(`SuiClient::new_ws_client`).  
See our [JSON-RPC](json-rpc.md#sui-json-rpc-methods) doc for the list of available methods.

> Note: As of [Sui version 0.6.0](https://github.com/MystenLabs/sui/releases/tag/devnet-0.6.0), the WebSocket client is for [subscription only](pubsub.md); use the HTTP client for other API methods.

## References

Find the `rustdoc` output for key Sui projects at:

* Sui blockchain - https://mystenlabs.github.io/sui/
* Narwhal and Tusk consensus engine - https://mystenlabs.github.io/narwhal/
* Mysten Labs infrastructure - https://mystenlabs.github.io/mysten-infra/

## Configuration
Add the `sui-sdk` crate in your [`Cargo.toml`](https://doc.rust-lang.org/cargo/reference/manifest.html) file like so:
```toml
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui" }
```
If you are connecting to the devnet, use the `devnet` branch instead:
```toml
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui", branch = "devnet" }
```

## Examples

### Example 1 - Get all objects owned by an address

This will print a list of object summaries owned by the address `"0xec11cad080d0496a53bafcea629fcbcfff2a9866"`:

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

You can verify the result with the [Sui Explorer](https://explorer.devnet.sui.io/) if you are using the Sui Devnet Gateway.

### Example 2 - Create and execute transaction

Use this example to conduct a transaction in Sui using the Sui Devnet Gateway:

```rust
use std::str::FromStr;
use sui_sdk::{
    crypto::SuiKeystore,
    types::{
        base_types::{ObjectID, SuiAddress},
        crypto::Signature,
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
    let keystore = SuiKeystore::load_or_create(&keystore_path)?;
    let signer = keystore.signer(my_address);

    // Sign the transaction
    let transaction = Transaction::from_data(transfer_tx, &signer)

    // Execute the transaction
    let transaction_response = sui
        .execute_transaction(transaction)
        .await?;

    println!("{:?}", transaction_response);

    Ok(())
}
```

### Example 3 - Event subscription

Use the the WebSocket client to [subscribe to events](pubsub.md).

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
> Note: You will need to connect to a fullnode for the Event subscription service, see [Fullnode setup](fullnode.md#fullnode-setup) if you want to run a Sui Fullnode.


## Larger examples

See the Sui Rust SDK README for the [Tic Tac Toe](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk) example.
