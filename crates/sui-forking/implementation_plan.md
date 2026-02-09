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

## Part 2: Compose Existing Stores for Forking

Use the **existing** `FileSystemStore`, `DataStore`, and `ReadThroughStore`
from `sui-data-store` — no new traits or store structs needed in `sui-data-store`.
The `ForkingStore` in `sui-forking` holds two separate references:

1. **Bare `FileSystemStore`** — for transactions (no fallback, returns None on miss)
2. **`ReadThroughStore<FileSystemStore, DataStore>`** — for objects (FileSystemStore
   primary with GraphQL fallback and write-back on miss)

Both share the same `FileSystemStore` instance (via `Arc`).

### Directory Layout

All data for a fork lives in a single directory tree. There is no sharing
between forks — each fork is fully isolated. The root directory is
user-specified, with `<chain_id>/<forked_at_checkpoint>/` as the next levels.
This allows multiple forks from different networks and checkpoints to coexist.

The `FileSystemStore` already provides this layout under its `base_path`:

```
<root>/
  <chain_id>/
    <forked_at_checkpoint>/
      transaction/
        <tx_digest>                            BCS: TransactionFileData (data + effects + checkpoint)
      epoch/
        <epoch_id>                             BCS: EpochFileData
      objects/
        <object_id_hex>/
          <version>                            BCS: Object
          root_versions                        CSV: version mappings
          checkpoint_versions                  CSV: checkpoint mappings
```

This is the existing `FileSystemStore` directory structure. No changes needed.

### How it works

**Transactions** — `ForkingStore` calls `FileSystemStore` directly via the
existing `TransactionStore` trait. Adapter code in `ForkingStore` bridges
the API mismatch:
- `TransactionDigest` → `.to_string()` for lookups
- `TransactionInfo` (bundled data+effects+checkpoint) ↔ separate SimulatorStore methods
- Events stored as a separate BCS file alongside (not part of `TransactionInfo`)

**Objects** — `ForkingStore` calls `ReadThroughStore<FileSystemStore, DataStore>`
via the existing `ObjectStore` trait:
- Object hit on disk → return immediately
- Object miss → GraphQL fetch with `AtCheckpoint(forked_at_checkpoint)` →
  write back to FileSystemStore → return
- Cached objects survive restarts

**Checkpoints, committees, events** — remain in-memory in `ForkingStore` for
now (same as current code). Will be moved to filesystem later.

---

## Part 3: `ForkingStore` Refactor in `sui-forking`

### New ForkingStore struct

```rust
pub struct ForkingStore {
    // Transactions: FileSystemStore directly, no fallback
    fs_store: Arc<FileSystemStore>,

    // Objects: FileSystemStore + GraphQL fallback with write-back
    object_store: Arc<ReadThroughStore<FileSystemStore, DataStore>>,

    // Checkpoints (in-memory for now, move to filesystem later)
    checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    checkpoint_digest_to_sequence_number: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,

    // Committees (in-memory for now)
    epoch_to_committee: Vec<Committee>,

    // Events (in-memory for now — not part of TransactionInfo)
    events: HashMap<TransactionDigest, TransactionEvents>,

    forked_at_checkpoint: u64,
}
```

Transactions and objects are filesystem-backed. Checkpoints, committees, and
events remain in-memory for now.

**No** `live_objects`, `objects`, or `transactions` maps — these are fully
delegated to the filesystem stores.

### Instantiation (in `sui-forking` server startup)

```rust
use sui_data_store::Node;
use sui_data_store::stores::{DataStore, FileSystemStore, ReadThroughStore};

// 1. Create the GraphQL-backed store (only used as object fallback)
let node = match chain {
    Chain::Mainnet => Node::Mainnet,
    Chain::Testnet => Node::Testnet,
    _ => todo!(),
};
let graphql = DataStore::new(node.clone(), version)?;

// 2. Create the filesystem store pointed at the fork directory
let fs = Arc::new(FileSystemStore::new(node)?);

// 3. Compose: filesystem primary + GraphQL secondary for objects
let object_store = Arc::new(ReadThroughStore::new(
    FileSystemStore::new(node)?,
    graphql,
));

// 4. Create ForkingStore and plug into Simulacrum
let forking_store = ForkingStore::new(
    &config.genesis, fs, object_store, forked_at_checkpoint,
);
let simulacrum = Simulacrum::new_with_network_config_store(&config, rng, forking_store);
```

`ReadThroughStore` handles the object read-through: FileSystemStore miss →
GraphQL fetch with `AtCheckpoint(forked_at_checkpoint)` → write back to
FileSystemStore. Cached objects survive restarts.

### SimulatorStore implementation

ForkingStore implements `SimulatorStore` with mixed delegation:

```rust
impl SimulatorStore for ForkingStore {
    // --- Checkpoints: in-memory (unchanged from current code) ---
    fn get_checkpoint_by_sequence_number(&self, seq: CheckpointSequenceNumber)
        -> Option<VerifiedCheckpoint>
    {
        self.checkpoints.get(&seq).cloned()
    }

    // --- Transactions: delegate to fs_store with adapter ---
    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        // Adapter: TransactionDigest.to_string() for lookup key
        // TransactionInfo.data (TransactionData) needs wrapping
        todo!("adapter from TransactionInfo to VerifiedTransaction")
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.fs_store.transaction_data_and_effects(&digest.to_string())
            .ok()?
            .map(|info| info.effects)
    }

    // --- Objects: delegate to object_store (ReadThroughStore) ---
    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        let key = ObjectKey {
            object_id: *id,
            version_query: VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        };
        self.object_store.get_objects(&[key])
            .ok()?
            .into_iter()
            .next()?
            .map(|(obj, _version)| obj)
    }

    // --- Events: in-memory ---
    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.events.get(digest).cloned()
    }

    // ... etc for all SimulatorStore methods
}
```

### Other trait implementations

ForkingStore also needs: `ObjectStore`, `BackingPackageStore`, `ChildObjectResolver`,
`ParentSync`, `GetModule`, `ModuleResolver`, `ReadStore`.

Object lookups go through `self.object_store` (ReadThroughStore).
Same pattern as current ForkingStore but backed by filesystem + GraphQL.

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

1. **Transaction adapter**: `TransactionStore` returns `TransactionData` (unsigned),
   but `SimulatorStore::get_transaction` returns `VerifiedTransaction` (with
   signatures). Need to determine if `TransactionInfo` can carry a
   `VerifiedTransaction` or if a separate file is needed for the full signed tx.

2. **Error handling**: SimulatorStore methods return `Option<T>` (no Result).
   The filesystem store returns `Result<Option<T>>`. ForkingStore bridges
   this by unwrapping/logging errors.

3. **FileSystemStore base path**: The existing `FileSystemStore::new(node)`
   resolves its base path from environment variables. Need to ensure the fork
   directory `<root>/<chain_id>/<checkpoint>/` is properly set (may need a
   `new_with_base_path()` constructor or env var override).

---

## Files to Create/Modify

### sui-data-store
| File | Action |
|------|--------|
| No changes needed initially | Use existing traits and stores as-is |

### sui-forking
| File | Action |
|------|--------|
| `src/store/mod.rs` | Rewrite ForkingStore: add fs_store + object_store, remove transactions/live_objects/objects maps, adapter code |
| `src/store/rpc_data_store.rs` | Update to create the two-reference composition |
| `src/server/mod.rs` | Update initialization code |
| `src/execution.rs` | Simplify — object read-through is automatic |

---

## Verification Plan

1. `cargo check -p sui-forking` compiles cleanly
2. Boot server, execute a transaction, verify transaction data appears in
   `transaction/` directory on disk
3. Verify objects fetched from GraphQL appear in `objects/` directory
4. Restart server and verify cached data loads correctly
