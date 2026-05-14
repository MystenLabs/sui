# Disclaimer

This is highly experimental tooling intended for development and testing purposes only. It is not recommended for production use and is provided as-is without guarantees.

Expect breaking changes until this is officially released and stable.

# Sui Forking Tool

A development tool that enables testing and developing against a local Sui network initialized with state from mainnet, testnet, or devnet at a specific checkpoint.

## Overview

`sui-fork` allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the live Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Develop locally and test contracts that interact with real deployed packages
- Have full control over checkpoint and time progression to simulate different scenarios

> [!NOTE]
> Unlike a standard local Sui network with validators, the forking tool runs in lock-step mode where each transaction is executed sequentially and creates a checkpoint.
> That means that you have full control over the advancement of checkpoints, time, (and soon epochs too) to simulate different scenarios.

## Quick Start

#### 1. Build or install `sui-fork`

From the Sui workspace root:

```bash
cargo build -p sui-fork
```

> [!NOTE]
> The examples below assume `sui-fork` is on your `PATH`. If you are using the
> workspace build directly, replace `sui-fork` with
> `./target/debug/sui-fork`.

#### 2. Start the fork

In a terminal, run `sui-fork start` with the desired flags.

**From latest checkpoint on mainnet**

```bash
sui-fork start
```

> [!TIP]
> By default, if no flags are specified, the fork starts from mainnet at the latest known checkpoint.
> The fork serves Sui gRPC on `127.0.0.1:9000` by default.

**From latest checkpoint on testnet**

```bash
sui-fork start --network testnet
```

> [!TIP]
> Supported networks are `mainnet`, `testnet`, and `devnet`. The default is `mainnet`.

**From a specific checkpoint on mainnet**

```bash
sui-fork start --checkpoint 12345678
```

**From a specific checkpoint on testnet with custom data directory**

```bash
sui-fork start --network testnet --checkpoint 12345678 --data-dir /tmp/sui-fork-demo
```

> [!NOTE]
> Local resume state is stored under
> `{data-dir}/{network}/forked_at_{checkpoint}`. When you restart the same
> fork, reuse the same `--data-dir`, `--network`, and `--checkpoint`.

#### 3. Confirm the fork is reachable

In another terminal, check the fork status:

```bash
sui-fork status
```

#### 4. Add the fork as a Sui CLI environment

```bash
sui client new-env --alias local-fork --rpc http://127.0.0.1:9000
sui client switch --env local-fork
```

You can now use Sui CLI commands such as `sui client ptb`,
`sui client publish`, `sui client upgrade`, or other read/write commands against the forked network.

> [!NOTE]
> Use `--forking-mode` on transaction commands when you need to impersonate a
> sender on the local fork. Note that this is not available yet for `sui client ptb`, only for regular write commands.

#### 5. Control checkpoint and time progression

```bash
sui-fork advance-checkpoint
sui-fork advance-clock --duration-ms 1000
sui-fork status
```

> [!TIP]
> If your test depends on address-owned objects at startup, add repeatable
> `--address 0x...` or `--object 0x...` flags to `sui-fork start`.

## Impersonating Senders

The Sui CLI supports `--forking-mode` on transaction commands such as
`sui client upgrade`. This flag is only intended for local forked networks. It
submits the transaction with an empty signature list, which tells the forked
network to execute the transaction as the declared sender without requiring that
sender's private key.

Transactions with non-empty signatures still use normal signature verification.

## Forked Network vs Sui CLI Local Network

The table below summarizes when to use each option:

| Topic | Local forked network (`sui-fork`) | Sui CLI local network |
| --- | --- | --- |
| Initial state | Starts from real chain state (mainnet/testnet/devnet) at a chosen checkpoint | Starts from a fresh genesis state (or from an existing one on disk) |
| Existing on-chain packages and objects | Available from the fork point (fetched/cached on demand) | Not available unless you deploy/create them locally |
| External dependency at runtime | Needs network access to source chain for first-time object fetches | Fully local once started |
| Execution model | Single validator, lock-step, sequential execution | Multi-validator local network flow |
| Checkpoint/time/epoch control | Explicit control through `advance-checkpoint`, `advance-clock` | Driven by normal local network progression |
| Best for | Testing against real deployed packages and realistic chain state | Fast local development from clean state |
| Startup cost | Higher (state bootstrap + potential object downloads) | Lower (local genesis and startup) |
| Determinism/reproducibility | Deterministic from selected checkpoint + seeded objects | Deterministic from local genesis/configuration |

## Seeding Owned Objects

Owned-object enumeration can be seeded when starting the fork:

```bash
sui-fork start --checkpoint 12345678 --address 0x... --object 0x...
```

- `--data-dir <path>` is the exact filesystem root for local fork data.
  Objects, checkpoints, transactions, indices, and `seed_manifest.json` are
  written directly under that directory.
- Without `--data-dir`, the default root is
  `{base_path}/{network}/forked_at_{checkpoint}`.
- `SUI_FORK_DATA` sets `{base_path}` directly.
- On Unix, the default `{base_path}` is `$XDG_DATA_HOME/sui_fork_data` when
  `XDG_DATA_HOME` is set, otherwise `$HOME/.sui_fork_data`.
- On Windows, the default `{base_path}` is `%APPDATA%/sui_fork_data`.
- `--address` is repeatable and seeds metadata for every object owned by that
  address at the fork checkpoint. Address seeding requires a checkpoint in the
  GraphQL object enumeration range, which is usually a range within the last hour.
- `--object` is repeatable and fetches that object at the fork checkpoint. If
  the object is address-owned, it is added to the initial owned-object index.
- Fork metadata is written once to `seed_manifest.json` under the fork
  directory. The manifest is immutable and records the source network, original
  fork checkpoint, and any optional seed object metadata. When no seed inputs
  are provided, it is still written with an empty seed entry list.

When restarting with the same fork data directory, omit seed flags. If a seed
manifest already exists and any seed flags are provided, startup fails instead
of overwriting or reinterpreting the local state. Resume uses the original fork
checkpoint from `seed_manifest.json`, starts from the highest locally persisted
checkpoint, and keeps the durable owned-object index and deleted-object markers
authoritative over the manifest seed entries.

## Limitations
- Sequential execution: Transactions are executed one at a time, no parallelism.
- Simulating transactions is currently not supported, so automatic gas estimation is not available via CLI or SDKs. All transactions require explicit gas budget.
- Staking and related operations are not supported.
- Single validator, single authority network.
- Object fetching overhead: First access to objects requires network download.
- Forking from a checkpoint older than 1 hour requires explicit object seeding (you need to know which owned objects you want to have pulled at startup)
- Owned-object enumeration only covers locally materialized post-fork state; it is not a full inventory of every object an address owned at the fork checkpoint.
- Objects deleted or wrapped after the fork point are no longer available through direct current-ID lookup, but exact historical versions remain readable when available.
- If it forks at checkpoint X, you cannot depend on objects created after checkpoint X from the actual real network. You'll need to restart the network at that checkpoint or a later one.
- Not recommended for parallel test execution since all transactions are executed sequentially on a single validator.
