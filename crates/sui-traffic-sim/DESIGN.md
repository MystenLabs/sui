# Sui Traffic Simulator Design

## Overview
A transaction generation and submission system for the Sui blockchain that simulates realistic network traffic patterns through multiple actors, transaction clients, and RPC clients interacting with various Move applications.

## Core Components

### 1. Actor
- Represents an individual entity with a unique Sui wallet address
- Manages multiple transaction clients and RPC clients
- Maintains account state and balance

### 2. Transaction Client
- Generates transactions based on configured patterns
- Submits transactions to the Sui network
- Handles transaction signing and gas management
- Reports metrics (success/failure rates, latency)

### 3. RPC Client  
- Reads on-chain data written by transactions
- Verifies transaction effects
- Monitors chain state changes
- Collects performance metrics

### 4. Move Applications
- Collection of Move packages in subdirectories
- Each app represents different transaction patterns and use cases
- Apps are deployed to the network and interacted with by transaction clients

## Architecture

```
┌─────────────────────────────────────────────┐
│           Traffic Simulator                 │
├─────────────────────────────────────────────┤
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │         Actor Manager                │   │
│  └──────────┬──────────────────────────┘   │
│             │                               │
│      ┌──────▼──────┐                       │
│      │   Actor 1   │                       │
│      ├─────────────┤                       │
│      │ Wallet Addr │                       │
│      ├─────────────┤                       │
│      │ ┌─────────┐ │                       │
│      │ │ Tx Client│ │                       │
│      │ └─────────┘ │                       │
│      │ ┌─────────┐ │                       │
│      │ │RPC Client│ │                       │
│      │ └─────────┘ │                       │
│      └─────────────┘                       │
│                                             │
│      [Actor 2...N]                         │
│                                             │
└─────────────────┬───────────────────────────┘
                  │
                  ▼
         ┌────────────────┐
         │  Sui Network   │
         └────────────────┘
                  ▲
                  │
         ┌────────┴────────┐
         │  Move Apps      │
         ├─────────────────┤
         │ • App 1        │
         │ • App 2        │
         │ • App N        │
         └─────────────────┘
```

## Transaction Flow

1. **Initialization**: Actor Manager creates actors with configured wallets
2. **Client Creation**: Each actor spawns transaction and RPC clients
3. **Transaction Generation**: Tx clients generate transactions based on patterns
4. **Submission**: Transactions are signed and submitted to Sui
5. **Verification**: RPC clients read and verify on-chain effects
6. **Metrics Collection**: Performance data is aggregated

## Directory Structure

```
sui-traffic-sim/
├── src/
│   ├── actor/
│   │   ├── mod.rs
│   │   └── manager.rs
│   ├── clients/
│   │   ├── transaction.rs
│   │   └── rpc.rs
│   ├── config/
│   │   └── mod.rs
│   └── main.rs
├── apps/
│   ├── app_1/
│   │   └── Move.toml
│   ├── app_2/
│   │   └── Move.toml
│   └── .../
├── Cargo.toml
└── DESIGN.md
```

## Configuration

```toml
[simulator]
num_actors = 100
transactions_per_second = 1000

[actor]
tx_clients_per_actor = 5
rpc_clients_per_actor = 2

[apps]
enabled = ["app_1", "app_2", ...]
weights = { app_1 = 0.4, app_2 = 0.3, ... }
```

## Key Design Decisions

1. **Decoupled Architecture**: Actors, clients, and apps are loosely coupled for flexibility
2. **Async/Concurrent**: Leverages Tokio for high-throughput concurrent operations
3. **Configurable Load**: Transaction patterns and rates are fully configurable
4. **Metrics First**: Built-in observability for performance analysis
5. **Extensible Apps**: Easy to add new Move packages for different scenarios