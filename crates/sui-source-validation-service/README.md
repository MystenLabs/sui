# Sui Source Validation Service

This document describes the Sui Source Validation Service. It is engineering documentation primarily for engineers who may want to build, extend, configure, or understand the service.

The Source Validation Service is a server that returns Move source code associated with on-chain Move bytecode. It fetches and builds Move source code for a repository, and then verifies that the built artifact matches the on-chain bytecode. 

The default configuration limits scope to Sui framework packages in `crates/sui-framework/packages`:

- `move-stdlib` — [address `0x1`](https://suiexplorer.com/object/0x1)
- `sui-framework` — [address `0x2`](https://suiexplorer.com/object/0x2)
- `sui-system` — [address `0x3`](https://suiexplorer.com/object/0x2)
- `deepbook` — [address `0xdee9`](https://suiexplorer.com/object/0xdee9)

See examples below for requesting source from the server.

## Build and Run

```
cargo run --release --bin sui-source-validation-service crates/sui-source-validation-service/config.toml 
```

See [`config.toml` in this directory](config.toml).

## Configuring

A sample configuration entry is as follows:

```toml
[[packages]]
source = "Repository"
[packages.values]
repository = "https://github.com/mystenlabs/sui"
branch = "framework/mainnet"
network = "mainnet"
packages = [
    { path = "crates/sui-framework/packages/deepbook", watch = "0xdee9" },
    { path = "crates/sui-framework/packages/move-stdlib", watch = "0x1" },
    { path = "crates/sui-framework/packages/sui-framework", watch = "0x2" },
    { path = "crates/sui-framework/packages/sui-system", watch = "0x3" },
]
```

It specifies the `repository` and `branch` for one or more move `packages`. `network` specifies the on-chain network to verify the source against. It can be one of `mainnet`, `testnet`, `devnet`, or `localnet`.

A package `path` specifies the path of the package in the repository (where the `Move.toml` is).
The `watch` field is optional, and specifies the address of an object that the server should monitor for on-chain changes if a package is upgraded. For example, Sui framework packages mutate their on-chain address when upgraded. 
Non-framework packages may mutate an `UpgradeCap` or an object wrapping the `UpgradeCap` (in which case, `watch` should be set to the `UpgradeCap` object ID or wrapped object ID respectively).

Currently the `watch` field intends only to invalidate and evict the source code if on-chain code changes via upgrades. Due to current limitations, it does not automatically attempt to find and reprocess the latest source code. To reprocess the latest source code, restart the server, which will download and verify the source code afresh.

The `HOST_PORT` environment variable sets the server host and port. The default is `0.0.0.0:8000`.

## Usage

After running `cargo run --bin sui-source-validation-service crates/sui-source-validation-service/config.toml` locally, try:

```
curl 'http://0.0.0.0:8000/api?address=0x2&module=coin&network=mainnet' --header 'X-Sui-Source-Validation-Version: 0.1'
```

This returns the source code for module `coin` on `mainnet` where the package `address` is `0x2` in JSON, e.g., `{"source":"..."}`.

For errors, or if the source code does not exist, an error encoded in JSON returns, e.g., `{"error":"..."}`.

The URL parameters `address`, `module`, and `network` are required.

Although not required, it is good practice to set the `X-Sui-Source-Validation-Version` header.
