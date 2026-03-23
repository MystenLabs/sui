# Sui Data for Forking

Multi-tier caching data store for Sui blockchain data.

This crate provides a flexible data store abstraction for retrieving and caching
Sui blockchain data (transactions, epochs, objects). The stores are loosely modeled
after the GraphQL schema in `crates/sui-indexer-alt-graphql/schema.graphql`.

## Capability Traits

- `TransactionStore` / `TransactionStoreWriter`
- `EpochStore` / `EpochStoreWriter`
- `ObjectStore` / `ObjectStoreWriter`
- `CheckpointStore` / `CheckpointStoreWriter`

`ReadDataStore` and `ReadWriteDataStore` remain convenience bundles for the
transaction/epoch/object capability set.

## Store Implementations

| Store | Description | Read | Write |
|-------|-------------|------|-------|
| `DataStore` | Remote GraphQL-backed store (mainnet/testnet) | Yes | No |
| `FileSystemStore` | Persistent local disk cache | Yes | Yes |
| `InMemoryStore` | Unbounded in-memory cache | Yes | Yes |
| `LruMemoryStore` | Bounded LRU cache | Yes | Yes |
| `ReadThroughStore` | Read-through cache over a source | Yes | Primary only |
| `WriteThroughStore` | Hot cache over a writable backing store | Yes | Yes |
| `ForkingStore` | Routes each capability to a different chain | Yes | Yes |

## Composition Primitives

`ReadThroughStore<Primary, Secondary>`
- Reads `Primary` first, falls back to `Secondary`, and caches successful misses into `Primary`.
- Direct writes update `Primary` only.

`WriteThroughStore<Primary, Secondary>`
- Reads `Primary` first, falls back to `Secondary`, and caches successful misses into `Primary`.
- Direct writes update `Secondary` first, then `Primary`.

`ForkingStore<Tx, Epoch, Obj, Ckpt>`
- Routes each capability to its dedicated chain.
- It is a router, not a search-order combinator.

## Composition Examples

```rust
use sui_data_store::{
    Node,
    stores::{
        ForkingStore, DataStore, FileSystemStore, InMemoryStore, ReadThroughStore,
        WriteThroughStore,
    },
};

// Filesystem -> GraphQL for object reads, persisting successful misses to disk.
let graphql = DataStore::new(Node::Mainnet, "test-version")?;
let disk = FileSystemStore::new(Node::Mainnet)?;
let disk_then_graphql = ReadThroughStore::new(disk, graphql);

// In-memory -> filesystem for generic writable caching.
let memory = InMemoryStore::new(Node::Mainnet);
let disk = FileSystemStore::new(Node::Mainnet)?;
let hot_mem_fs = WriteThroughStore::new(memory, disk);

// Route different capabilities to different chains.
let transactions = hot_mem_fs;
let epochs = /* another chain or the same chain */;
let objects = /* e.g. WriteThroughStore<InMemoryStore, ReadThroughStore<FileSystemStore, DataStore>> */;
let checkpoints = /* another chain or the same chain */;
let store = ForkingStore::new(transactions, epochs, objects, checkpoints);
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
