# Sui Data for Forking

Multi-tier caching data store for Sui blockchain data.

This crate provides a flexible data store abstraction for retrieving and caching
Sui blockchain data. The trait surface covers transactions, epochs, objects,
and checkpoints, and the filesystem/GraphQL store implementations now support
the object-fetching paths used by `sui-forking`.

## Capability Traits

- `TransactionStore` / `TransactionStoreWriter`
- `EpochStore` / `EpochStoreWriter`
- `ObjectStore` / `ObjectStoreWriter`
- `LatestObjectStore`
- `CheckpointStore` / `CheckpointStoreWriter`

`ReadDataStore` and `ReadWriteDataStore` remain convenience bundles for the
transaction/epoch/object capability set.

## Store Implementations

| Store | Description | Read | Write |
|-------|-------------|------|-------|
| `GraphQLStore` | Remote GraphQL-backed epoch/checkpoint/object store | Yes | No |
| `FileSystemStore` | Persistent local epoch/checkpoint/object cache | Yes | Yes |
| `InMemoryStore` | Unbounded in-memory epoch/checkpoint cache | Yes | Yes |
| `LruMemoryStore` | Bounded LRU epoch/checkpoint cache | Yes | Yes |
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
use forking_data_store::{
    Node,
    stores::{
        ForkingStore, FileSystemStore, GraphQLStore, InMemoryStore, ReadThroughStore,
        WriteThroughStore,
    },
};

// Filesystem -> GraphQL for checkpoint/object reads, persisting successful misses to disk.
let graphql = GraphQLStore::new(Node::Mainnet, "test-version")?;
let disk = FileSystemStore::new(Node::Mainnet)?;
let disk_then_graphql = ReadThroughStore::new(disk, graphql);

// In-memory -> filesystem for writable checkpoint caching.
let memory = InMemoryStore::new(Node::Mainnet);
let disk = FileSystemStore::new(Node::Mainnet)?;
let hot_mem_fs = WriteThroughStore::new(memory, disk);

// Route different capabilities to different chains.
let transactions = FileSystemStore::new(Node::Mainnet)?;
let epochs = disk_then_graphql;
let objects = FileSystemStore::new(Node::Mainnet)?;
let checkpoints = /* another chain or the same chain */;
let store = ForkingStore::new(transactions, epochs, objects, checkpoints);
```

## Object Storage Layout

`FileSystemStore` keeps objects under the same per-chain, optionally per-fork,
directory that checkpoints and epochs use:

```text
<chain-or-fork>/
  objects/
    <object_id>/
      <version>
      root_versions
      checkpoint_versions
```

- `<version>` stores the BCS-encoded `sui_types::object::Object`.
- `root_versions` maps `RootVersion(max_version)` queries to the concrete stored version.
- `checkpoint_versions` maps `AtCheckpoint(checkpoint)` queries to the concrete stored version.

This lets `sui-forking` answer three object query kinds consistently:

- `Version(v)`
- `RootVersion(max_v)`
- `AtCheckpoint(checkpoint)`

## Network Configuration

Use the `Node` enum to configure which network to connect to:

```rust
use sui_data_store::Node;

let mainnet = Node::Mainnet;
let testnet = Node::Testnet;
let custom = Node::Custom("https://my-rpc.example.com".to_string());
```
