# Disclaimer

This is highly experimental tooling intended for development and testing purposes only. It is not recommended for production use and is provided as-is without guarantees.

Expect breaking changes until this is officially released and stabilized.

# Sui Forking Tool

A development tool that enables testing and developing against a local Sui network initialized with state from mainnet, testnet, or devnet at a specific checkpoint.

## Overview

`sui-forking` allows developers to start a local network in lock-step mode and execute transactions against initial state derived from the actual Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Test contracts that interact with real deployed packages
- Develop locally while maintaining consistency with production state
- Run integration tests against forked network state

:important:
Unlike a standard local Sui network with validators, the forking tool runs in lock-step mode where each transaction is executed sequentially and creates a checkpoint.
That means that you have full control over the advancement of checkpoints, time, and epochs to simulate different scenarios.

## Usage

### Starting a Local Forked Network

Start a local forked network at the latest checkpoint:

```bash
sui-forking start --network testnet
```

This command:
- Starts a local network on port 8123 (default)
- Allows you to execute transactions against this local state and fetches objects on-demand from the real network

#### Options

- `--checkpoint <number>`: The checkpoint to fork from (required)
- `--network <network>`: Network to fork from: `mainnet`, `testnet`, or `devnet` (required). Local network is not currently supported.
- `--port <port>`: Port for the local network (default: 8123)


## Available Commands

Once the forked network is running, you can use these commands:

### Faucet - request SUI tokens

```bash
sui-forking faucet --address <address> --amount <amount>
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

## Limitations

- Sequential execution: Transactions are executed one at a time, no parallelism.
- Staking and related operations are not supported.
- One validator, single authority network.
- Object fetching overhead: First access to objects requires network download
- If it forks at checkpoint X, you cannot depend on objects created after checkpoint X. You'll need to restart the network at that checkpoint or a later one.
- Currently, it does not save state. You will lose all changes when you stop the local network.
- It requires Postgres DB for storing the local network state; a `sui-indexer-alt` DB is needed.

## Related Tools

e `sui-replay-2`: A generic data store implementation for downloading and caching objects from the RPC.
- `simulacrum`: Local network execution in lock-step mode

