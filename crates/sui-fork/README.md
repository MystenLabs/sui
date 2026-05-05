# Disclaimer

This is highly experimental tooling intended for development and testing purposes only. It is not recommended for production use and is provided as-is without guarantees.

Expect breaking changes until this is officially released and stable.

# Sui Forking Tool

A development tool that enables testing and developing against a local Sui network initialized with state from mainnet, testnet, or devnet at a specific checkpoint.

## Overview

`sui-fork` allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the live Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Test contracts that interact with real deployed packages
- Develop locally while maintaining consistency with production state
- Run integration tests against forked network state and using packages deployed on the real live network

**Important Note**
Unlike a standard local Sui network with validators, the forking tool runs in lock-step mode where each transaction is executed sequentially and creates a checkpoint.
That means that you have full control over the advancement of checkpoints, time, and epochs to simulate different scenarios.

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

- `--address` is repeatable and seeds metadata for every object owned by that
  address at the fork checkpoint. Address seeding requires a checkpoint in the
  GraphQL object enumeration range.
- `--object` is repeatable and fetches that object at the fork checkpoint. If
  the object is address-owned, it is added to the initial owned-object index.
- Seed metadata is written once to `seed_manifest.json` under the fork
  directory. The manifest is immutable.

When restarting with the same `--data-dir`, `--network`, and `--checkpoint`,
omit seed flags. If a seed manifest already exists and any seed flags are
provided, startup fails instead of overwriting or reinterpreting the local
state. Resume starts from the highest locally persisted checkpoint and keeps
the durable owned-object index and deleted-object markers authoritative over
the original seed manifest.

## Limitations
- Staking and related operations are not supported.
- Single validator, single authority network.
- Object fetching overhead: First access to objects requires network download.
- Forking from a checkpoint older than 1 hour requires explicit object seeding (you need to know which owned objects you want to have pulled at startup)
- Owned-object enumeration only covers locally materialized post-fork state; it is not a full inventory of every object an address owned at the fork checkpoint.
- Objects deleted or wrapped after the fork point are no longer available through direct current-ID lookup, but exact historical versions remain readable when available.
- If it forks at checkpoint X, you cannot depend on objects created after checkpoint X from the actual real network. You'll need to restart the network at that checkpoint or a later one.
- Sequential execution: Transactions are executed one at a time, no parallelism.
