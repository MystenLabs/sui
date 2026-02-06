# Implementation Plan: Filesystem-backed Storage for Forking/Simulacrum

## Goal

Design a clean data layer in `sui-data-store` that can serve as the storage backend
for `sui-forking`. All reads and writes go through the filesystem (no in-memory caching).
`ForkingStore` in `sui-forking` wraps this and implements `SimulatorStore`.

---

## Part 1: New Traits in `sui-data-store`

Add new traits in `sui-data-store/src/lib.rs` (or a new `forking.rs` module) that map
to the data model `SimulatorStore` needs. These are separate from the existing
`TransactionStore`/`ObjectStore` traits which serve the GraphQL-caching use case.

### CheckpointStore / CheckpointStoreWriter

```rust
pub trait CheckpointStore {
    fn get_checkpoint_by_sequence_number(
        &self, seq: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>>;

    fn get_checkpoint_by_digest(
        &self, digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>>;

    fn get_highest_checkpoint(&self) -> Result<Option<VerifiedCheckpoint>>;

    fn get_checkpoint_contents(
        &self, digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>>;
}

pub trait CheckpointStoreWriter: CheckpointStore {
    fn write_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()>;
    fn write_checkpoint_contents(&self, contents: &CheckpointContents) -> Result<()>;
}
```

### ExecutedTransactionStore / ExecutedTransactionStoreWriter

Named `Executed*` to distinguish from the existing `TransactionStore` (which uses
string digests and returns `TransactionInfo`). These work with the types simulacrum
produces.

```rust
pub trait ExecutedTransactionStore {
    fn get_transaction(
        &self, digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>>;

    fn get_transaction_effects(
        &self, digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>>;

    fn get_transaction_events(
        &self, digest: &TransactionDigest,
    ) -> Result<Option<TransactionEvents>>;
}

pub trait ExecutedTransactionStoreWriter: ExecutedTransactionStore {
    fn write_transaction(&self, tx: &VerifiedTransaction) -> Result<()>;
    fn write_transaction_effects(
        &self, digest: &TransactionDigest, effects: &TransactionEffects,
    ) -> Result<()>;
    fn write_transaction_events(
        &self, digest: &TransactionDigest, events: &TransactionEvents,
    ) -> Result<()>;
}
```

### VersionedObjectStore / VersionedObjectStoreWriter

The existing `ObjectStore` uses `ObjectKey` + `VersionQuery` (for caching network data).
This new trait works with the simpler `(ObjectID, SequenceNumber)` model that
simulacrum uses.

All object versions are kept on disk. Each object directory has a `latest` file
that tracks the current version number, updated on every write to that object.
This gives O(1) lookup for `get_object(id)`.

```rust
pub trait VersionedObjectStore {
    /// Get the latest version of an object by reading its `latest` file
    /// to find the current version, then loading that version.
    fn get_object(&self, id: &ObjectID) -> Result<Option<Object>>;

    /// Get an object at a specific version.
    fn get_object_at_version(
        &self, id: &ObjectID, version: SequenceNumber,
    ) -> Result<Option<Object>>;

    /// Get all objects owned by an address.
    fn owned_objects(
        &self, owner: SuiAddress,
    ) -> Result<Vec<Object>>;
}

pub trait VersionedObjectStoreWriter: VersionedObjectStore {
    /// Write objects to disk. Each written object is stored at
    /// objects/<object_id>/<version> and its `latest` file is updated
    /// to point to the new version.
    fn update_objects(
        &self,
        written: BTreeMap<ObjectID, Object>,
    ) -> Result<()>;
}
```

### CommitteeStore / CommitteeStoreWriter

```rust
pub trait CommitteeStore {
    fn get_committee_by_epoch(&self, epoch: EpochId) -> Result<Option<Committee>>;
}

pub trait CommitteeStoreWriter: CommitteeStore {
    fn write_committee(&self, committee: &Committee) -> Result<()>;
}
```

### Composite trait

```rust
pub trait ForkingDataStore:
    CheckpointStore + ExecutedTransactionStore + VersionedObjectStore + CommitteeStore {}

pub trait ForkingDataStoreReadWrite:
    ForkingDataStore
    + CheckpointStoreWriter
    + ExecutedTransactionStoreWriter
    + VersionedObjectStoreWriter
    + CommitteeStoreWriter {}
```

---

## Part 2: Filesystem Implementation in `sui-data-store`

New file: `sui-data-store/src/stores/forking_fs.rs`

### Directory Layout

The base directory is **user-specified** via a CLI argument in `sui-forking`.
This allows stopping and restarting the forked network from where it left off.
The top-level folder is the `<forked_at_checkpoint>` number, making it clear
which fork point this data belongs to.

```
<user-specified-dir>/
  <forked_at_checkpoint>/
    checkpoints/
      by_seq/<sequence_number>          BCS: inner checkpoint data (*)
      contents/<contents_digest_hex>    BCS: CheckpointContents
      digest_to_seq.csv                 CSV: <digest_hex>,<sequence_number>
      highest                           plain text: highest sequence number
    transactions/
      data/<tx_digest_hex>              BCS: inner Transaction data (*)
      effects/<tx_digest_hex>           BCS: TransactionEffects
      events/<tx_digest_hex>            BCS: TransactionEvents
    objects/
      <object_id_hex>/
        latest                          plain text: current version number
        <version>                       BCS: Object
        ...
    committees/
      <epoch>                           BCS: Committee
```

(*) For `VerifiedCheckpoint` and `VerifiedTransaction`, we serialize the inner
`CertifiedCheckpointSummary` / `Transaction` via BCS. On deserialization, we wrap
with `VerifiedCheckpoint::new_unchecked()` / `VerifiedTransaction::new_unchecked()`
since we trust data we wrote ourselves.

### Struct

```rust
pub struct ForkingFileStore {
    base_dir: PathBuf,  // <user-dir>/<forked_at_checkpoint>/
    forked_at_checkpoint: u64,
    /// Optional fallback for fetching objects not on local disk (e.g. from
    /// mainnet/testnet). On a get_object miss, the fallback is queried with
    /// AtCheckpoint(forked_at_checkpoint). The result is written to disk
    /// before returning, so subsequent reads are served from the filesystem.
    /// The fallback implements the existing sui_data_store::ObjectStore trait,
    /// so the 3-tier ReadThroughStore<LruMemory, ReadThroughStore<FileSystem,
    /// DataStore>> can be plugged in directly.
    fallback: Option<Box<dyn crate::ObjectStore + Send + Sync>>,
}

impl ForkingFileStore {
    pub fn new(base_dir: PathBuf, forked_at_checkpoint: u64) -> Result<Self> { ... }

    pub fn with_fallback(
        self,
        fallback: Box<dyn crate::ObjectStore + Send + Sync>,
    ) -> Self { ... }

    // Internal helpers (reuse patterns from existing FileSystemStore)
    fn read_bcs_file<T: DeserializeOwned>(&self, path: &Path) -> Result<T>;
    fn write_bcs_file<T: Serialize>(&self, path: &Path, data: &T) -> Result<()>;
}
```

### Key implementation details

**Fallback policy:** Only objects have a network fallback (GraphQL DataStore).
All other data (checkpoints, transactions, effects, events, committees) is
local-only — if not on disk, return `None`.

**Checkpoints:**
- `write_checkpoint`: Serialize to `checkpoints/by_seq/<seq>`, append to
  `digest_to_seq.csv`, update `highest` file.
- `get_checkpoint_by_digest`: Read `digest_to_seq.csv` (or cache it), look up
  sequence number, then read by sequence.
- `get_highest_checkpoint`: Read `highest` file, then read that checkpoint.
- No fallback: local-only.

**Transactions:**
- Three separate files per transaction (data, effects, events) keyed by digest hex.
- Simple: `write_transaction` → `transactions/data/<digest>`,
  `write_transaction_effects` → `transactions/effects/<digest>`, etc.
- No fallback: local-only.

**Committees:**
- One file per epoch: `committees/<epoch>`.
- No fallback: local-only.

**Objects:**
- `update_objects`: For each written object, write BCS to
  `objects/<id>/<version>` and update `objects/<id>/latest` with the new
  version number. All versions are kept on disk.
- `get_object`: Read `objects/<id>/latest` to get the current version number,
  then read `objects/<id>/<version>`. On miss (no `latest` file), if a
  fallback is configured, query it with
  `ObjectKey { object_id, version_query: AtCheckpoint(forked_at_checkpoint) }`.
  If found, write the object to disk + update `latest`, then return it.
  This means network-fetched objects are cached locally for future reads
  and survive restarts.
- `get_object_at_version`: Read `objects/<id>/<version>` directly.
- `owned_objects`: Scan all object directories, read each object's latest
  version, filter by owner. (Slow but functional; can optimize later with
  an owner-index file.)

---

## Part 3: `ForkingStore` Refactor in `sui-forking`

### New ForkingStore struct

```rust
pub struct ForkingStore {
    /// Filesystem-backed store from sui-data-store
    fs_store: ForkingFileStore,
}
```

No in-memory maps. All reads/writes go through `fs_store`.

### Instantiation (in `sui-forking` server startup)

```rust
use sui_data_store::Node;
use sui_data_store::stores::{DataStore, ForkingFileStore};

// 1. Create the GraphQL-backed store (only used as object fallback)
let node = match chain {
    Chain::Mainnet => Node::Mainnet,
    Chain::Testnet => Node::Testnet,
    _ => todo!(),
};
let graphql = DataStore::new(node, version)?;

// 2. Create the filesystem store with GraphQL fallback for objects
let fs_store = ForkingFileStore::new(user_specified_dir, forked_at_checkpoint)?
    .with_fallback(Box::new(graphql));

// 3. Create ForkingStore wrapping the filesystem store
let forking_store = ForkingStore::new(&config.genesis, fs_store);

// 4. Plug into Simulacrum
let simulacrum = Simulacrum::new_with_network_config_store(&config, rng, forking_store);
```

`DataStore` implements `sui_data_store::ObjectStore`, so it plugs directly into
`ForkingFileStore::with_fallback()`. No LRU or extra caching layers needed —
`ForkingFileStore` itself is the filesystem cache. Once an object is fetched
from GraphQL, it's written to disk and subsequent reads are served locally.
On restart with the same directory, previously fetched objects won't hit
GraphQL again.

### SimulatorStore implementation

ForkingStore implements `SimulatorStore` by delegating to the filesystem store:

```rust
impl SimulatorStore for ForkingStore {
    fn get_checkpoint_by_sequence_number(&self, seq: CheckpointSequenceNumber)
        -> Option<VerifiedCheckpoint>
    {
        self.fs_store.get_checkpoint_by_sequence_number(seq)
            .ok()
            .flatten()
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.fs_store.write_checkpoint(&checkpoint)
            .expect("failed to write checkpoint");
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.fs_store.get_object(id).ok().flatten()
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let digest = *effects.transaction_digest();
        self.fs_store.write_transaction(&transaction).unwrap();
        self.fs_store.write_transaction_effects(&digest, &effects).unwrap();
        self.fs_store.write_transaction_events(&digest, &events).unwrap();
        self.fs_store.update_objects(written_objects).unwrap();
    }

    // ... etc for all SimulatorStore methods
}
```

### Other trait implementations

ForkingStore also needs: `ObjectStore`, `BackingPackageStore`, `ChildObjectResolver`,
`ParentSync`, `GetModule`, `ModuleResolver`, `ReadStore`.

These delegate to `self.fs_store` for object lookups. Same pattern as current
ForkingStore but going through filesystem instead of HashMaps.

---

## Part 4: gRPC Service Updates

The gRPC services (`ForkingLedgerService`, `ForkingTransactionExecutionService`)
currently access data via `Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>`. This
pattern stays the same - the services don't need to know about the filesystem
store. They call `sim.store()` / `sim.store_mut()` which returns `&ForkingStore` /
`&mut ForkingStore`, and the SimulatorStore methods handle the rest.

No changes needed to the gRPC service layer itself.

---

## Open Questions / Considerations

1. **Checkpoint digest→seq lookup**: Loading the full `digest_to_seq.csv` on every
   lookup is wasteful. Consider lazy-loading into memory (like the existing
   FileSystemStore does with root_versions_map). This is a small deviation from
   "no in-memory caching" but it's just an index, not data.

2. **owned_objects performance**: Scanning all object directories and loading
   each `latest` version to check the owner is O(n). An owner-index file could
   help but adds complexity. Fine for initial implementation.

3. **Serialization of Verified types**: Need to verify that
   `VerifiedCheckpoint`/`VerifiedTransaction` can roundtrip through BCS. If not,
   serialize the inner type and use `new_unchecked()` on deserialize.

4. **Error handling**: SimulatorStore methods return `Option<T>` (no Result).
   The filesystem store returns `Result<Option<T>>`. ForkingStore bridges
   this by unwrapping/logging errors.

---

## Files to Create/Modify

### sui-data-store
| File | Action |
|------|--------|
| `src/lib.rs` | Add new traits (or `mod forking;`) |
| `src/stores/forking_fs.rs` | New: ForkingFileStore implementation |
| `src/stores/mod.rs` | Export new module and types |
| `Cargo.toml` | Possibly no changes (sui-types already a dep) |

### sui-forking
| File | Action |
|------|--------|
| `src/store/mod.rs` | Rewrite ForkingStore to wrap ForkingFileStore |
| `src/store/checkpoint_store.rs` | Remove (logic moves to sui-data-store) |
| `src/store/object_store.rs` | Remove (logic moves to sui-data-store) |
| `src/store/rpc_data_store.rs` | Possibly remove or simplify |
| `src/context.rs` | Update if ForkingStore constructor changes |
| `src/server/mod.rs` | Update initialization code |
| `src/execution.rs` | Update fetch_and_cache_object_from_rpc |

---

## Verification Plan

1. Unit tests in sui-data-store for ForkingFileStore:
   - Write/read roundtrip for checkpoints, transactions, objects, committees
   - get_highest_checkpoint after multiple writes
   - update_objects with writes and deletes
   - owned_objects filtering

2. Integration: boot sui-forking, execute a transaction, verify data appears on
   filesystem, restart and verify data loads correctly.
