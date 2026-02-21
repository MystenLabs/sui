# Disclaimer

This is highly experimental tooling intended for development and testing purposes only. It is not recommended for production use and is provided as-is without guarantees.

Expect breaking changes until this is officially released and stable.

# Sui Forking Tool

A development tool that enables testing and developing against a local Sui network initialized with state from mainnet, testnet, or devnet at a specific checkpoint.

## Overview

`sui-forking` allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the live Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Test contracts that interact with real deployed packages
- Develop locally while maintaining consistency with production state
- Run integration tests against forked network state and using packages deployed on the real live network

**Important Note**
Unlike a standard local Sui network with validators, the forking tool runs in lock-step mode where each transaction is executed sequentially and creates a checkpoint.
That means that you have full control over the advancement of checkpoints, time, and epochs to simulate different scenarios.

## Limitations
- Staking and related operations are not supported.
- Single validator, single authority network.
- Object fetching overhead: First access to objects requires network download.
- Forking from a checkpoint older than 1 hour requires explicit object seeding (you need to know which owned objects you want to have pulled at startup)
- If it forks at checkpoint X, you cannot depend on objects created after checkpoint X from the actual real network. You'll need to restart the network at that checkpoint or a later one.
- Sequential execution: Transactions are executed one at a time, no parallelism.

## Usage

### Build from source
To build the `sui-forking` tool from source, ensure you have Rust and Cargo installed, then run:

```bash
git clone https://github.com/MystenLabs/sui.git
cd sui/crates/sui-forking
cargo build
```

Now use the `sui-forking` binary located in `sui/target/debug/sui-forking`.

### Programmatic Usage (Rust)

`sui-forking` also exposes a library API for starting and controlling a local fork in-process.

```rust
use sui_forking::{ForkingNetwork, ForkingNode, ForkingNodeConfig, StartupSeeding};

# async fn run() -> anyhow::Result<()> {
let config = ForkingNodeConfig::builder()
    .network(ForkingNetwork::Testnet)
    .server_port(9001)
    .rpc_port(9000)
    .startup_seeding(StartupSeeding::None)
    .build()?;

let node = ForkingNode::start(config).await?;
let client = node.client();
let status = client.status().await?;
println!("checkpoint={} epoch={}", status.checkpoint, status.epoch);

node.shutdown().await?;
# Ok(())
# }
```

### Starting a Local Forked Network

Start a local forked network at the latest checkpoint:

```bash
sui-forking start --network testnet
```

This command:
- Starts a local "server" on port 9001 (default) - this is used to interact with the forked network, e.g., advance-checkpoints, request gas, advance-clock, advance-epoch, etc. You can do so via the CLI commands with the `sui-forking` binary or via the REST API (see below).
- Starts a local Sui network on port 9000 (default) - this is the gRPC endpoint you can connect the Sui client to interact with the network. 
- Allows you to execute transactions against this local state and fetches objects on-demand from the real network

#### Options

- `--checkpoint <number>`: The checkpoint to fork from (required)  - note that this is WIP
- `--network <network>`: Network to fork from: `mainnet`, `testnet`, `devnet`, or a custom one (`--network <CUSTOM-GRAPHQL-ENDPOINT> --fullnode-url <URL>`

### Old checkpoint seeding (`--accounts` vs `--objects`)

When you provide `--checkpoint`, startup seeding supports two exclusive modes:

- `--accounts`: discover owned objects through GraphQL at startup time for checkpoints not older than 1h.
- `--objects`: provide explicit object IDs to prefetch at startup time, required for checkpoints older than 1h.

`--accounts` and `--objects` are mutually exclusive.

Examples:

```bash
# Recent checkpoint (<=1h), account-based startup seeding
sui-forking start --network testnet --checkpoint 123456 --accounts 0x123...,0xabc...
```

```bash
# Old checkpoint (>1h), explicit object seeding
sui-forking start --network testnet --checkpoint 123456 --objects 0xabc...,0xdef...
```

## Available Commands

Once the forked network is running, you can use these commands:

### Faucet - request SUI tokens

```bash
sui-forking faucet --address <address> --amount <amount> # Max is 10M SUI
```

### Advance Checkpoint

```bash
sui-forking advance-checkpoint
```

Advances the checkpoint of the local network by 1.

### Advance Clock

```bash
sui-forking advance-clock
```

Advances the clock of the local network by 1 second.

### Advance Epoch

```bash
sui-forking advance-epoch
```

Advances the epoch of the local network by 1.

### Check Status

```bash
sui-forking status
```

Shows the current checkpoint, epoch, and number of transactions.

## Basic Use Case

1. Start the forked network:

```bash
sui-forking start --network testnet
```

2. In another terminal, request SUI tokens from the faucet:

```
sui client new-env --rpc-url http://127.0.0.1:3000 --alias fork

sui client switch --env fork
```

3. Request tokens:

```bash
sui-forking faucet --address <your-address> --amount 1000
```

4. Check balance
```bash
sui client gas
```

5. Call a package from testnet (e.g., the `@potatoes/ascii` package):
```bash
sui client ptb --move-call 0x65d106ccd0feddc4183dcaa92decafd3376ee9b34315aae938dc838f6d654f18::ascii::is_ascii '"hello"' --gas-budget 5000000
```

## Server REST API
The local forked network server exposes a REST API for interaction. The server listens on port 3001 by default.
### Endpoints
- `POST /advance-checkpoint`: Advance the checkpoint by 1
- `POST /advance-clock [seconds]`: Advance the clock by seconds (default: 1s if omitted).
- `POST /advance-epoch`: Advance the epoch by 1
- `POST /faucet`: Request SUI tokens
  - Body: `{ "address": "<address>", "amount": <amount> }`
- `GET /status`: Get current checkpoint, epoch, and transaction count

## Related Tools

e `sui-replay-2`: A generic data store implementation for downloading and caching objects from the RPC.
- `simulacrum`: Local network execution in lock-step mode
