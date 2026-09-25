This crate provides the wallet and client configuration layer that the Sui CLI and the tooling in this repository use to talk to a Sui network. Auto-generated documentation for this crate is [here](https://mystenlabs.github.io/sui/sui_sdk/index.html).

The JSON-RPC client this crate used to provide was removed together with the fullnode JSON-RPC service. For a general-purpose Rust SDK, use the [`sui-rust-sdk`](https://github.com/MystenLabs/sui-rust-sdk) crate, which talks to Sui over gRPC and GraphQL.

## What the crate provides

- `wallet_context::WalletContext`: a keystore-backed wallet that signs transactions and executes them against the active network environment over gRPC.
- `sui_client_config::SuiClientConfig`: the persisted client configuration (`client.yaml`), including the named network environments and the active address.
- `verify_personal_message_signature`: verification of personal message signatures through a fullnode.

## Getting started

Add the `sui-sdk` dependency as follows:

```toml
sui-sdk = { git = "https://github.com/mystenlabs/sui", package = "sui-sdk" }
tokio = { version = "1.2", features = ["full"] }
anyhow = "1.0"
```

The following example loads the wallet configuration written by the Sui CLI and reads the active address's gas coins through the gRPC client of the active environment:

```rust
use sui_sdk::sui_client_config::SuiClientConfig;
use sui_sdk::wallet_context::WalletContext;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_path = sui_config::sui_config_dir()?.join(sui_config::SUI_CLIENT_CONFIG);
    let mut wallet = WalletContext::new(&config_path)?;

    let address = wallet.active_address()?;
    let gas_objects = wallet.gas_objects(address).await?;
    println!("Address {address} owns {} gas objects", gas_objects.len());

    let client = wallet.grpc_client()?;
    println!("Reference gas price: {}", client.get_reference_gas_price().await?);
    Ok(())
}
```

## Documentation for sui-sdk crate

[GitHub Pages](https://mystenlabs.github.io/sui/sui_sdk/index.html) hosts the generated documentation for all Rust crates in the Sui repository.

### Building documentation locally

You can also build the documentation locally. To do so,

1. Clone the `sui` repo locally. Open a Terminal or Console and go to the `sui/crates/sui-sdk` directory.

1. Run `cargo doc` to build the documentation into the `sui/target` directory. Take note of location of the generated file from the last line of the output, for example `Generated /Users/foo/sui/target/doc/sui_sdk/index.html`.

1. Use a web browser, like Chrome, to open the `.../target/doc/sui_sdk/index.html` file at the location your console reported in the previous step.

## License

[SPDX-License-Identifier: Apache-2.0](https://github.com/MystenLabs/sui/blob/main/LICENSE)
