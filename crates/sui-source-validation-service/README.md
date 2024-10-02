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

## Hosted Service

Mysten Labs maintains a backend service hosted at `https://source.mystenlabs.com` for verified packages. The following example usages are available via the API:

- List current indexed sources via `curl 'https://source.mystenlabs.com/api/list'`. 
  - For example, to see all verified sources on `mainnet`, query the `mainnet` member: `curl 'https://source.mystenlabs.com/api/list' --header 'X-Sui-Source-Validation-Version: 0.1'  | jq .mainnet`
  
- Get verified source for a `(address, module, network)` triple via a request like `curl 'https://source.mystenlabs.com/api?address=0x2&module=coin&network=mainnet' --header 'X-Sui-Source-Validation-Version: 0.1'`

**Production Usage and Best Practices Tips*

On occasion `https://source.mystenlabs.com` may return a `502` response or experience downtime. When using `https://source.mystenlabs.com` to determine source authenticity, it is recommended interface with the service in such a way that it can robustly handle a non-`200` response or downtime. For example, when a non-`200` response is received, a tab showing verified source may temporarily show a message like `"Not available at this time"` when used in an explorer. Once the source verification resumes, requests will continue to allow clients to show source as normal.

The source service may experience transient downtime for at least the following reasons:

- RPC event subscription disconnection or instability. The Mysten source service actively monitors on-chain upgrade events to ensure it always reports accurate verified source. If RPC subscription is lost, the service will attempt to regain the connection. During the time of disconnection the service will not respond with verified source in order to preserve integrity. This behavior is especially important for Sui framework packages that are upgraded _in-place_ (e.g., `0x1`, `0x2`, `0x3`, and `0xdee9`) to ensure integrity. This is usually a transient issue.

- The on-chain package content has changed (e.g., due to a protocol upgrade) and the source repository does not yet reflect the new on-chain bytecode. 
  - This can happen when the branch containing we track for the source-to-be-verified as diverged from on-chain bytecode, or does not yet correspond to the new on-chain bytecode. This is especially the case for Sui framework packages that are upgraded _in-place_ at protocol upgrades (e.g., `0x1`, `0x2`, `0x3`, and `0xdee9`).
    - While usually transient, there may be extended periods of mismatched source and bytecode due to Sui's release process.
	
- A new version of Move compiler is released, requiring service redeployment.
  - For example, when framework packages are upgraded and require a more recent compiler version, the Mysten source service will need to be redeployed and will experience transient downtime.
