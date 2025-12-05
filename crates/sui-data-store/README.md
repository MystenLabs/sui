# sui-data-store

Multi-tier caching data store for Sui blockchain data.

This crate provides a flexible data store abstraction for retrieving and caching
Sui blockchain data (transactions, epochs, objects). The stores are loosely modeled
after the GraphQL schema in `crates/sui-indexer-alt-graphql/schema.graphql`.

## Core Traits

- `TransactionStore` - Retrieve transaction data and effects by digest
- `EpochStore` - Retrieve epoch information and protocol configuration
- `ObjectStore` - Retrieve objects by their keys with flexible version queries

The read traits above have corresponding writer traits (`TransactionStoreWriter`,
`EpochStoreWriter`, `ObjectStoreWriter`) for stores that support write-back caching.

## Store Implementations

| Store | Description | Read | Write |
|-------|-------------|------|-------|
| `DataStore` | Remote GraphQL-backed store (mainnet/testnet) | Yes | No |
| `FileSystemStore` | Persistent local disk cache | Yes | Yes |
| `InMemoryStore` | Unbounded in-memory cache | Yes | Yes |
| `LruMemoryStore` | Bounded LRU cache | Yes | Yes |
| `ReadThroughStore` | Composable two-tier caching pattern | Yes | Yes* |

\* `ReadThroughStore` delegates writes to its secondary (backing) store.

## Architecture

The typical 3-tier cache composition: Memory → FileSystem → GraphQL

```
┌─────────────────────────────────────────────────────────────────┐
│                        Client Code                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              ReadThroughStore<Memory, Inner>                    │
│      Fast in-memory cache (LruMemoryStore or InMemoryStore)     │
└─────────────────────────────────────────────────────────────────┘
                              │ cache miss
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│           ReadThroughStore<FileSystem, Remote>                  │
│         Persistent disk cache (FileSystemStore)                 │
└─────────────────────────────────────────────────────────────────┘
                              │ cache miss
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    DataStore (GraphQL)                          │
│              Remote data source (mainnet/testnet)               │
└─────────────────────────────────────────────────────────────────┘
```

## Composition Examples

Use `ReadThroughStore<Primary, Secondary>` to compose cache layers:

```rust
use sui_data_store::{Node, stores::{DataStore, LruMemoryStore, ReadThroughStore, FileSystemStore}};

// Full 3-tier: Memory → FileSystem → GraphQL (typical production setup)
let graphql = DataStore::new(Node::Mainnet);
let disk = FileSystemStore::new(Node::Mainnet)?;
let disk_with_remote = ReadThroughStore::new(disk, graphql);
let memory = LruMemoryStore::new(Node::Mainnet);
let store = ReadThroughStore::new(memory, disk_with_remote);

// 2-tier: Memory + FileSystem (e.g., CI testing with pre-populated disk cache)
let disk = FileSystemStore::new(Node::Mainnet)?;
let memory = LruMemoryStore::new(Node::Mainnet);
let store = ReadThroughStore::new(memory, disk);
```

## Version Queries

The `ObjectStore` trait supports three query modes via `VersionQuery`:

- `Version(v)` - Request object at exact version `v`
- `RootVersion(v)` - Request object at version `<= v` (for dynamic field roots)
- `AtCheckpoint(c)` - Request object as it existed at checkpoint `c`

## Network Configuration

Use the `Node` enum to configure which network to connect to:

```rust
use sui_data_store::Node;

let mainnet = Node::Mainnet;
let testnet = Node::Testnet;
let custom = Node::Custom("https://my-rpc.example.com".to_string());
```
