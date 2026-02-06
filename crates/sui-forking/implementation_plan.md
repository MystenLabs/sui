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

## Part 2: Extend `FileSystemStore` + New `ObjectReadThroughStore` Combinator

No new store struct. Instead:
1. Add the new traits as additional implementations on the **existing `FileSystemStore`**
2. Add a `FileSystemStore::new_with_path(base_dir)` constructor for user-specified directories
3. Add a new **`ObjectReadThroughStore<P, S>`** combinator that does read-through
   **only for objects**, passing everything else through to the primary

### Directory Layout

Object data and fork-specific state are stored separately:

**Shared object storage** — uses the existing `FileSystemStore` path
(`~/.sui_data_store/<chain_id>/objects/`). Object BCS files are immutable
and keyed by `(object_id, version)`, so they're safely shared across forks.
Objects already cached by previous runs are automatically available.

**Fork-specific storage** — user-specified directory, organized by
`<chain_id>/<forked_at_checkpoint>/`. This allows multiple forks from
different networks (mainnet, testnet) and different checkpoints to coexist.

```
~/.sui_data_store/<chain_id>/objects/          SHARED (existing FS store path)
  <object_id_hex>/
    <version>                                  BCS: Object

<user-specified-dir>/
  <chain_id>/
    <forked_at_checkpoint>/                    FORK-SPECIFIC
      object_versions/
        <object_id_hex>                        plain text: latest version for this fork
      checkpoints/
        by_seq/<sequence_number>               BCS: inner checkpoint data (*)
        contents/<contents_digest_hex>         BCS: CheckpointContents
        digest_to_seq.csv                      CSV: <digest_hex>,<sequence_number>
        highest                                plain text: highest sequence number
      transactions/
        data/<tx_digest_hex>                   BCS: inner Transaction data (*)
        effects/<tx_digest_hex>                BCS: TransactionEffects
        events/<tx_digest_hex>                 BCS: TransactionEvents
      committees/
        <epoch>                                BCS: Committee
```

(*) For `VerifiedCheckpoint` and `VerifiedTransaction`, we serialize the inner
`CertifiedCheckpointSummary` / `Transaction` via BCS. On deserialization, we wrap
with `VerifiedCheckpoint::new_unchecked()` / `VerifiedTransaction::new_unchecked()`
since we trust data we wrote ourselves.

### FileSystemStore changes

Add a new constructor and implement the new traits. The store needs two paths:
- `shared_objects_dir`: the existing `~/.sui_data_store/<chain_id>/objects/`
  for shared object BCS files
- `fork_dir`: `<user-dir>/<chain_id>/<forked_at_checkpoint>/` for fork-specific
  data (checkpoints, transactions, committees, object_versions)

```rust
impl FileSystemStore {
    /// Create a FileSystemStore for forking with explicit paths.
    pub fn new_for_forking(
        shared_objects_dir: PathBuf,  // existing objects path
        fork_dir: PathBuf,            // <user-dir>/<chain_id>/<checkpoint>/
    ) -> Result<Self> { ... }
}

// New trait implementations on the existing FileSystemStore:
impl CheckpointStore for FileSystemStore { ... }
impl CheckpointStoreWriter for FileSystemStore { ... }
impl ExecutedTransactionStore for FileSystemStore { ... }
impl ExecutedTransactionStoreWriter for FileSystemStore { ... }
impl VersionedObjectStore for FileSystemStore { ... }
impl VersionedObjectStoreWriter for FileSystemStore { ... }
impl CommitteeStore for FileSystemStore { ... }
impl CommitteeStoreWriter for FileSystemStore { ... }
```

The existing `read_bcs_file`/`write_bcs_file` helpers are reused internally.
The existing traits (`TransactionStore`, `ObjectStore`, etc.) remain unchanged.

### `ObjectReadThroughStore<P, S>` combinator

New file: `sui-data-store/src/stores/object_read_through.rs`

```rust
/// A combinator that does read-through ONLY for objects.
/// All other traits (checkpoints, transactions, committees) pass through
/// to the primary with no fallback.
pub struct ObjectReadThroughStore<P, S> {
    primary: P,               // e.g. FileSystemStore — read/write everything
    secondary: S,             // e.g. DataStore (GraphQL) — read-only, objects only
    forked_at_checkpoint: u64,
}
```

**Trait implementations:**

```rust
// OBJECTS: read-through (primary → miss → secondary → write-back to primary)
impl<P, S> VersionedObjectStore for ObjectReadThroughStore<P, S>
where
    P: VersionedObjectStoreWriter,
    S: crate::ObjectStore,  // existing ObjectStore trait (get_objects with ObjectKey)
{
    fn get_object(&self, id: &ObjectID) -> Result<Option<Object>> {
        // Try primary
        if let Some(obj) = self.primary.get_object(id)? {
            return Ok(Some(obj));
        }
        // Fallback to secondary using AtCheckpoint query
        let key = ObjectKey {
            object_id: *id,
            version_query: VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        };
        let results = self.secondary.get_objects(&[key])?;
        if let Some((obj, _version)) = results.into_iter().next().flatten() {
            // Write back to primary filesystem
            self.primary.update_objects(BTreeMap::from([(*id, obj.clone())]))?;
            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }
    // get_object_at_version: primary only (no fallback)
    // owned_objects: primary only (no fallback)
}

// ALL OTHER TRAITS: pass-through to primary, no fallback
impl<P, S> CheckpointStore for ObjectReadThroughStore<P, S>
where P: CheckpointStore
{
    // delegates every method to self.primary
}

impl<P, S> ExecutedTransactionStore for ObjectReadThroughStore<P, S>
where P: ExecutedTransactionStore
{
    // delegates every method to self.primary
}

impl<P, S> CommitteeStore for ObjectReadThroughStore<P, S>
where P: CommitteeStore
{
    // delegates every method to self.primary
}

// Writer traits also pass through to primary
impl<P, S> CheckpointStoreWriter for ObjectReadThroughStore<P, S> ...
impl<P, S> ExecutedTransactionStoreWriter for ObjectReadThroughStore<P, S> ...
impl<P, S> VersionedObjectStoreWriter for ObjectReadThroughStore<P, S> ...
impl<P, S> CommitteeStoreWriter for ObjectReadThroughStore<P, S> ...
```

### Key implementation details

**Checkpoints (FileSystemStore):**
- `write_checkpoint`: Serialize to `checkpoints/by_seq/<seq>`, append to
  `digest_to_seq.csv`, update `highest` file.
- `get_checkpoint_by_digest`: Read `digest_to_seq.csv` (or cache it), look up
  sequence number, then read by sequence.
- `get_highest_checkpoint`: Read `highest` file, then read that checkpoint.

**Transactions (FileSystemStore):**
- Three separate files per transaction (data, effects, events) keyed by digest hex.
- `write_transaction` → `transactions/data/<digest>`,
  `write_transaction_effects` → `transactions/effects/<digest>`, etc.

**Committees (FileSystemStore):**
- One file per epoch: `committees/<epoch>`.

**Objects (FileSystemStore):**
- `update_objects`: For each written object, write BCS to the **shared** dir
  (`shared_objects_dir/<id>/<version>`) and update the **fork-specific**
  `object_versions/<id>` with the new version number.
- `get_object`: Read fork-specific `object_versions/<id>` to get the current
  version, then read shared `shared_objects_dir/<id>/<version>`. Returns
  `None` if no version file for this fork.
- `get_object_at_version`: Read `shared_objects_dir/<id>/<version>` directly.
- `owned_objects`: Scan fork-specific `object_versions/` directory, read each
  object's latest version from shared storage, filter by owner.

**Objects (ObjectReadThroughStore):**
- `get_object`: Primary miss → query secondary with
  `AtCheckpoint(forked_at_checkpoint)` → write back to primary → return.
  Network-fetched objects are cached on disk and survive restarts.

---

## Part 3: `ForkingStore` Refactor in `sui-forking`

### New ForkingStore struct

```rust
pub struct ForkingStore {
    /// ObjectReadThroughStore composes:
    /// - Primary: FileSystemStore (local disk, read/write everything)
    /// - Secondary: DataStore (GraphQL, read-only objects fallback)
    store: ObjectReadThroughStore<FileSystemStore, DataStore>,
}
```

No in-memory maps. All reads/writes go through the composed store.

### Instantiation (in `sui-forking` server startup)

```rust
use sui_data_store::Node;
use sui_data_store::stores::{DataStore, FileSystemStore, ObjectReadThroughStore};

// 1. Create the GraphQL-backed store (only used as object fallback)
let node = match chain {
    Chain::Mainnet => Node::Mainnet,
    Chain::Testnet => Node::Testnet,
    _ => todo!(),
};
let graphql = DataStore::new(node, version)?;

// 2. Create the filesystem store with two paths:
//    - shared objects: ~/.sui_data_store/<chain_id>/objects/ (existing, shared across forks)
//    - fork-specific: <user_dir>/<chain_id>/<checkpoint>/ (checkpoints, txns, latest pointers)
let shared_objects_dir = FileSystemStore::default_objects_dir(node)?;
let fork_dir = user_specified_dir
    .join(chain_id)
    .join(forked_at_checkpoint.to_string());
let fs = FileSystemStore::new_for_forking(shared_objects_dir, fork_dir)?;

// 3. Compose: filesystem primary + GraphQL secondary (objects only)
let store = ObjectReadThroughStore::new(fs, graphql, forked_at_checkpoint);

// 4. Create ForkingStore and plug into Simulacrum
let forking_store = ForkingStore::new(&config.genesis, store);
let simulacrum = Simulacrum::new_with_network_config_store(&config, rng, forking_store);
```

`DataStore` implements the existing `sui_data_store::ObjectStore` trait.
`ObjectReadThroughStore` uses it as the secondary for object fallback only.
Once an object is fetched from GraphQL, `FileSystemStore` writes it to the
**shared** objects directory, so it's available to any fork. The fork-specific
`object_versions/` pointer is also updated. On restart with the same fork
directory, all state is preserved.

### SimulatorStore implementation

ForkingStore implements `SimulatorStore` by delegating to the composed store:

```rust
impl SimulatorStore for ForkingStore {
    fn get_checkpoint_by_sequence_number(&self, seq: CheckpointSequenceNumber)
        -> Option<VerifiedCheckpoint>
    {
        self.store.get_checkpoint_by_sequence_number(seq)
            .ok()
            .flatten()
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.store.write_checkpoint(&checkpoint)
            .expect("failed to write checkpoint");
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        // This goes through ObjectReadThroughStore:
        // filesystem hit → return; miss → GraphQL → write to disk → return
        self.store.get_object(id).ok().flatten()
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let digest = *effects.transaction_digest();
        self.store.write_transaction(&transaction).unwrap();
        self.store.write_transaction_effects(&digest, &effects).unwrap();
        self.store.write_transaction_events(&digest, &events).unwrap();
        self.store.update_objects(written_objects).unwrap();
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
| `src/lib.rs` | Add new traits (CheckpointStore, ExecutedTransactionStore, etc.) |
| `src/stores/filesystem.rs` | Add `new_with_path()`, implement new traits on FileSystemStore |
| `src/stores/object_read_through.rs` | New: ObjectReadThroughStore combinator |
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

1. Unit tests in sui-data-store:
   - FileSystemStore new trait impls: write/read roundtrip for checkpoints,
     transactions, objects (with `latest` file), committees
   - ObjectReadThroughStore: primary hit (no secondary call), primary miss
     with secondary fallback + write-back, pass-through for non-object traits

2. Integration: boot sui-forking, execute a transaction, verify data appears on
   filesystem, restart and verify data loads correctly.
