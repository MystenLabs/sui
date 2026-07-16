Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0

# Implement DB-backed owned-object index

## Goal

Implement the `sui-fork` owned-object index as typed-store `DBMap` tables.

This should be treated as the first implementation of the owned-object index. The design should
preserve RPC semantics while keeping the persistence model DB-native and row-oriented.

## Review Feedback Incorporated

The PR review changed the plan in four important ways:

- The owned-object index should store lightweight index rows. Store the indexed object's version as
  the value and fetch object metadata from the local object store when serving RPC reads.
- Update the index using old and new object state, matching the consistent-store object indices:
  derive delete keys from input/current objects and put keys from output objects.
- Keep the DB open for the lifetime of the store instead of opening and closing it for each owned
  object operation. The synchronous constructor may use a temporary Tokio runtime for typed-store
  open; this is acceptable for now if isolated in one helper.
- Treat a second process opening the same DB as an error. Add context that the owned-object index
  directory may already be in use, especially on Windows where DB locking behavior matters.

## Relevant Files

- `crates/sui-fork/src/store.rs`
- `crates/sui-fork/Cargo.toml`
- `crates/sui-fork/src/owned_object_index.rs`
- `crates/sui-fork/src/filesystem.rs`
- `crates/sui-fork/src/tests/owned_object_index.rs`
- `crates/sui-fork/src/tests/store_execution.rs`

Follow `crates/.devx-style.md`: imports are grouped as std, external crates, crate; one item per
`use`; sorted within each group.

## RPC Requirements

The index backs `DataStore::get_owned_object_infos()` and must preserve the external behavior of
the v2 RPC owned-object iterator:

- address owner lookup
- cursor object ID, inclusive lower bound
- optional `StructTag` filter, including current wildcard behavior for filters with empty type
  parameters
- returned `OwnedObjectInfo` includes object type, balance, object ID, and version

## Proposed Ownership Shape

Prefer splitting the owned-object index into its own small store wrapper instead of embedding it
directly in `FilesystemStore`:

```rust
struct DataStoreInner {
    forked_at_checkpoint: CheckpointSequenceNumber,
    gql: GraphQLClient,
    local: FilesystemStore,
    owned_index: OwnedObjectIndexStore,
    local_snapshot_lock: RwLock<()>,
}
```

This keeps the dependency graph as:

```text
DataStore -> OwnedObjectIndexStore
          -> FilesystemStore
```

That is cleaner than `DataStore -> FilesystemStore -> OwnedObjectIndexStore`, because the DB index
can later be swapped independently from the filesystem object cache.

## DB Shape

Use metadata plus two owner scan tables:

```rust
#[derive(DBMapUtils)]
struct OwnedObjectIndexTables {
    meta: DBMap<(), OwnedObjectIndexMetadata>,
    by_owner: DBMap<OwnedObjectOwnerKey, SequenceNumber>,
    by_owner_type: DBMap<OwnedObjectOwnerTypeKey, SequenceNumber>,
}
```

Do not store duplicated object metadata in the DBMap values. The value is only the object version.
The local object store is the source of truth for object type, owner, balance, and digest.

The basic owner scan key is:

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct OwnedObjectOwnerKey {
    owner: SuiAddress,
    object_id: ObjectID,
}
```

This supports unfiltered owner pagination with object ID ordering:

```text
(owner_a, object_1) -> version_1
(owner_a, object_2) -> version_2
(owner_a, object_3) -> version_3
(owner_b, object_4) -> version_4
```

The typed owner scan key is:

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct OwnedObjectOwnerTypeKey {
    owner: SuiAddress,
    object_type: StructTag,
    object_id: ObjectID,
}
```

This supports exact type-filtered scans from `(owner, object_type, cursor_object_id)` to
`(owner, object_type, ObjectID::MAX)`.

Keep both tables:

- `by_owner` preserves owner-wide object ID ordering and supports wildcard type filters.
- `by_owner_type` avoids scanning every object for exact type filters.

For a filter like `0x2::coin::Coin` with empty type parameters, use `by_owner` and keep
`struct_tag_filter_matches()` because the empty type-parameter filter is a wildcard over all
instantiations. For a fully specified `StructTag`, use `by_owner_type`.

Do not use `DBMap<SuiAddress, Vec<...>>`: that would recreate the current whole-list
read/modify/write behavior for every owner update.

## Key Encoding and Ordering

Range scans depend on the on-disk byte order of keys matching the intended sort order. `DBMap`
serializes keys with `typed_store::be_fix_int_ser` (bincode, big-endian, fixed-int encoding), so:

- fixed-width byte arrays like `SuiAddress` and `ObjectID` sort by their raw bytes, which matches
  their `Ord` impls;
- unsigned integers sort ascending;
- struct keys sort lexicographically field by field.

This is the same order-preserving scheme that `sui-indexer-alt-consistent-store` implements by hand
in `src/db/key.rs`; here `DBMap` provides it, so the index does not need a custom key codec.

The keys above are safe because every component that varies within a scan is a fixed-width 32-byte
array, and `object_type` is pinned to a single value across the `by_owner_type` scan bounds. Keep it
that way: do not add a `Vec`/collection field (those sort by length first) or a signed integer to a
key, or range scans will break silently. The key structs intentionally do not derive `Ord` —
`DBMap` ordering comes from the encoding above, not from a Rust `Ord` impl, and the range APIs
(`safe_range_iter`, `safe_iter_with_bounds`) do not require it.

## Metadata

Use a persisted singleton metadata row to distinguish these states:

- never initialized
- initialized but empty
- initialized with data
- initialized with an unsupported schema version

```rust
const OWNED_OBJECT_INDEX_VERSION: u64 = 1;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct OwnedObjectIndexMetadata {
    version: u64,
}
```

`owned_object_index_exists()` should be fallible:

```rust
pub(crate) fn owned_object_index_exists(&self) -> anyhow::Result<bool> {
    match self.tables.meta.get(&())? {
        Some(metadata) if metadata.version == OWNED_OBJECT_INDEX_VERSION => Ok(true),
        Some(metadata) => bail!(
            "unsupported owned-object index version: {}",
            metadata.version,
        ),
        None => Ok(false),
    }
}
```

Write the metadata marker in the same batch as index rows, including for an empty index.

## Cargo Change

Add typed-store as a dependency:

```toml
typed-store.workspace = true
```

Keep `typed-store-error.workspace = true` because `RpcIndexes::owned_objects_iter` still returns
typed-store errors.

## Opening the DB

```rust
const OWNED_OBJECTS_INDEX_DB_DIR: &str = "owned_objects_db";
```

Open the DB once when constructing the owning store wrapper:

```rust
pub(crate) fn open(root: &Path) -> anyhow::Result<Self> {
    let path = root.join(INDICES_DIR).join(OWNED_OBJECTS_INDEX_DB_DIR);
    let tables = Self::open_tables(path).with_context(|| {
        format!(
            "failed to open owned-object index DB; another process may be using {}",
            root.display(),
        )
    })?;

    Ok(Self { tables })
}
```

typed-store starts Tokio-backed metrics tasks while opening RocksDB tables. Existing
`FilesystemStore::new_with_root()` and test constructors are synchronous, so isolate the temporary
runtime in the DB-open helper:

```rust
fn open_tables(path: PathBuf) -> anyhow::Result<OwnedObjectIndexTables> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Self::open_tables_at(path);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .context("failed to build Tokio runtime for owned-object index open")?;

    runtime.block_on(async move { Self::open_tables_at(path) })
}
```

`DataStore::new()` is already `async`, so the production path opens the DB inside an existing
runtime and takes the `Handle::try_current()` branch; the temporary runtime only fires for the
synchronous `#[cfg(test)]` constructors. That temporary runtime is dropped as soon as `block_on`
returns, which aborts the typed-store metrics sampling task it was created to host — so metrics
never actually run on that path. This is acceptable for a dev tool. If typed-store exposes a way to
open with metrics disabled (via `MetricConf`), prefer that and drop the temporary-runtime helper
entirely; check this before implementing.

Do not open and close the DB for each owned-object operation. RocksDB already provides the process
lock we want; repeated open/close adds complexity and makes lock failures happen later.

Before committing to `Arc<OwnedObjectIndexTables>`, check whether `DBMap` or the generated tables
type is cheap to clone. Use `Arc` only if the table handles are not cloneable or clone is not
intended to share the same underlying DB handle. `DBMap` already holds its RocksDB handle behind an
`Arc`, and `OwnedObjectIndexStore` lives inside `DataStoreInner`, which is itself wrapped in
`Arc<DataStoreInner>`, so an extra `Arc` is most likely unnecessary.

## Removing the File-Based Index

The file-based owned-object index is fully replaced, so remove it from `filesystem.rs`:

- `OwnedObjectEntry` and its `from_object()` constructor
- `OWNED_OBJECTS_INDEX_FILE` and `owned_objects_index_path()`
- `get_owned_object_entries()`, `apply_owned_object_index_updates()`,
  `write_owned_object_entries()`, `owned_object_index_exists()`
- the free functions `remove_owned_entry()` and `upsert_owned_entry()`
- the `indices/owned_objects` line in the module-level folder-structure doc comment

`store.rs` imports `OwnedObjectEntry` from `filesystem.rs`; that import and the seed-rebuild code
in `ensure_owned_object_index_initialized()` that builds `Vec<OwnedObjectEntry>` are replaced by the
`OwnedObjectIndexStore` calls described below.

`INDICES_DIR` stays in `filesystem.rs` but must be reachable from the new module — either make it
`pub(crate)` or move the directory-layout constants to a shared location.

### Migration of existing fork directories

Existing fork data directories contain an `indices/owned_objects` *file*; the new DB lives at the
`indices/owned_objects_db/` *directory*, so there is no path collision. Because
`owned_object_index_exists()` now consults the DB metadata row rather than a file on disk, an old
directory reports "not initialized" and is re-seeded automatically on first use. The stale
`indices/owned_objects` file is then orphaned; leaving it is harmless, but the implementation may
delete it opportunistically during open. Call this out so the behavior is intentional rather than
surprising.

## Index Row Helpers

The index only needs to derive keys and values from live address-owned objects:

```rust
fn owned_index_value(object: &Object) -> Option<(
    OwnedObjectOwnerKey,
    OwnedObjectOwnerTypeKey,
    SequenceNumber,
)> {
    let owner = match object.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
        _ => return None,
    };
    let object_type = object.struct_tag()?;
    let object_id = object.id();
    let version = object.version();

    Some((
        OwnedObjectOwnerKey { owner, object_id },
        OwnedObjectOwnerTypeKey {
            owner,
            object_type,
            object_id,
        },
        version,
    ))
}
```

The index should not define a persistent full-entry metadata struct. Convert an `Object` to
`OwnedObjectInfo` at the read boundary.

```rust
fn owned_object_info_from_object(object: &Object) -> Option<OwnedObjectInfo> {
    let owner = match object.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
        _ => return None,
    };
    let object_type = object.struct_tag()?;
    let object_ref = object.compute_object_reference();

    Some(OwnedObjectInfo {
        owner,
        object_type,
        balance: object.as_coin_maybe().map(|coin| coin.value()),
        object_id: object_ref.0,
        version: object_ref.1,
    })
}
```

## Atomic Updates

Use one `DBBatch` per logical index update. Calls such as `insert_batch()` and `delete_batch()`
build the in-memory batch; they do not make one table durable before the other. The DB is changed
only at `batch.write()`, so `by_owner`, `by_owner_type`, and `meta` must be written by the same
batch.

The update API should accept old and new object states, not only removed object IDs:

```rust
pub(crate) fn apply_owned_object_index_updates<'a>(
    &self,
    old_objects: impl IntoIterator<Item = &'a Object>,
    new_objects: impl IntoIterator<Item = &'a Object>,
) -> anyhow::Result<()> {
    let mut batch = self.tables.by_owner.batch();

    for object in old_objects {
        self.delete_object_from_batch(&mut batch, object)?;
    }

    for object in new_objects {
        self.put_object_in_batch(&mut batch, object)?;
    }

    self.mark_initialized(&mut batch)?;
    batch.write()?;
    Ok(())
}
```

`delete_object_from_batch()` derives both old keys from the input object and removes them from both
tables. `put_object_in_batch()` derives both new keys from the output object and inserts the object
version into both tables. Deletes must run before puts: when an object's key is unchanged (a plain
version bump) the delete and the put target the same key, and RocksDB applies batch operations in
order, so delete-then-put correctly collapses to the put. When owner or type changed, the old key
and new key differ, so the delete clears the stale row and the put writes the new one.

This mirrors the consistent-store handlers:

- objects present in inputs but not outputs emit a delete for the old key
- objects present in outputs emit a put for the new key
- objects whose owner or type changed emit a delete for the old key and a put for the new key

### Collecting Old Object State

`DataStore::apply_object_updates()` must collect the old state of every affected object **before**
it mutates local latest metadata — that is, before the `mark_object_as_*` calls and before
`write_live_object()`.

The old state needed is the object's **last live state**, which is exactly what
`FilesystemStore::get_latest_object(id)` returns *before* the removal marks and live writes are
applied. Use it for both removed and written objects:

- Removed objects: do **not** load by the removed `(object_id, version)`. `effects.deleted()`,
  `wrapped()`, and `unwrapped_then_deleted()` report the post-transaction lamport version with
  `OBJECT_DIGEST_DELETED`; no object file exists on disk at that version, so a version-keyed load
  returns `None` and the stale `(owner, object_id)` row would never be deleted. Load
  `get_latest_object(id)` instead.
- Written objects: `get_latest_object(id)` returns the previous live version before
  `write_live_object()` overwrites the `latest` pointer, so transfers and type changes can delete
  the previous key.

`get_latest_object()` returning `None` is expected and correct for:

- newly created objects (no previous key to delete);
- `UnwrappedThenDeleted` objects, which never existed as a live input and so were never indexed;
- objects whose previous state was already removed (their key was deleted when they were removed).

Because the collection step is a single uniform `get_latest_object()` over the union of written and
removed object IDs, `collect_owned_index_input_objects()` does not need to special-case removal
kind.

If reconstructing old keys from filesystem state proves fragile in practice, an explicit
`by_object: DBMap<ObjectID, (OwnedObjectOwnerKey, OwnedObjectOwnerTypeKey)>` table makes deletes
exact and independent of the object store. Unlike the consistent-store handlers — which receive
full input *and* output objects from the checkpoint — `apply_object_updates()` only receives object
*refs* for removals, so this table is a legitimate design choice, not just a fallback. Prefer the
`get_latest_object()` approach first; reach for `by_object` only if it is needed.

Then persist objects and metadata, filter out output objects whose latest state is deleted, and
update the index in the same local snapshot lock:

```rust
let old_objects = self.collect_owned_index_input_objects(&written_objects, &removed_objects)?;

// existing mark deleted/wrapped/unwrapped and write live objects...

let new_objects = written_objects
    .values()
    .filter(|object| self.output_object_should_be_indexed(object))
    .collect::<Vec<_>>();

self.inner
    .owned_index
    .apply_owned_object_index_updates(old_objects.iter(), new_objects)?;
```

If the first implementation cannot collect old objects reliably, keep a `by_object` helper table
temporarily, but treat it as a fallback. The preferred design derives stale keys from the old object
state and avoids another duplicated metadata table.

## Seeding the Index

For initial seed construction, write objects to the local object store first, then build the index
from those same objects:

```rust
pub(crate) fn replace_from_objects<'a>(
    &self,
    objects: impl IntoIterator<Item = &'a Object>,
) -> anyhow::Result<()> {
    let mut batch = self.tables.by_owner.batch();

    for entry in self.tables.by_owner.safe_iter() {
        let (key, _) = entry?;
        batch.delete_batch(&self.tables.by_owner, [key])?;
    }
    for entry in self.tables.by_owner_type.safe_iter() {
        let (key, _) = entry?;
        batch.delete_batch(&self.tables.by_owner_type, [key])?;
    }

    for object in objects {
        self.put_object_in_batch(&mut batch, object)?;
    }

    self.mark_initialized(&mut batch)?;
    batch.write()?;
    Ok(())
}
```

The exact iterator/error shape above may need adjustment for `safe_iter()`, but the important part
is that both tables are cleared and repopulated in one batch.

The per-key delete loop above is `O(n)` and inflates the batch. Prefer folding a single
`DBBatch::schedule_delete_range()` per table into the shared batch instead, so the clear stays in
the same atomic write without one batch entry per existing row. At expected seed sizes the per-key
loop is acceptable, but `schedule_delete_range()` is the cleaner shape.

## DataStore Reads

`owned_object_index_exists()` becomes fallible, so update initialization:

```rust
fn ensure_owned_object_index_initialized(&self) -> anyhow::Result<()> {
    if self.inner.owned_index.owned_object_index_exists()? {
        return Ok(());
    }

    let _local_snapshot_guard = self.write_local_snapshot()?;
    if self.inner.owned_index.owned_object_index_exists()? {
        return Ok(());
    }

    // existing checkpoint safety and seed-manifest rebuild logic...
}
```

Owner listing should scan index rows first, then materialize objects from the local object store:

```rust
let cursor_object_id = cursor.map(|cursor| cursor.object_id);

// Hold the read guard across both the index scan and object materialization so a concurrent
// executor write cannot land between scanning a row and reading the object it points at.
let _local_snapshot_guard = self.read_local_snapshot()?;
let rows = self
    .inner
    .owned_index
    .scan_owner(owner, object_type.as_ref(), cursor_object_id)
    .map_err(|e| StorageError::custom(e.to_string()))?;

let mut infos = Vec::new();
for row in rows {
    let object = self
        .inner
        .local
        .get_object_at_version(&row.object_id, row.version.value())
        .map_err(|e| StorageError::custom(e.to_string()))?
        .ok_or_else(|| StorageError::custom(format!(
            "owned-object index points to missing object {} version {}",
            row.object_id,
            row.version.value(),
        )))?;

    let Some(info) = owned_object_info_from_object(&object) else {
        continue;
    };
    if object_type
        .as_ref()
        .is_none_or(|filter| struct_tag_filter_matches(filter, &info.object_type))
    {
        infos.push(info);
    }
}

Ok(infos)
```

Prefer `FilesystemStore::get_object_at_version()` while holding the local snapshot lock so reads do
not fetch from remote RPC in the middle of materializing index results. Index rows should point only
at object versions already persisted locally.

This materializes every row from the cursor to the end of the owner's range eagerly, one object
file read per row. The current file-based index has the same unbounded shape — `owned_objects_iter`
returns an iterator with no limit pushed down — so this is not a regression. For owners with many
objects it is still `N` file reads per call. A lazy iterator that materializes per `next()` would
bound the work to the page size, but the returned iterator would then have to hold the read guard
for its lifetime. Treat the lazy iterator as a follow-up, not part of this change.

## Scan API

Return small rows from the index:

```rust
struct OwnedObjectIndexRow {
    object_id: ObjectID,
    version: SequenceNumber,
}
```

`scan_owner()` should use:

- `by_owner` when there is no type filter
- `by_owner_type` when the filter can be treated as an exact full-type match
- `by_owner` when the filter has empty type parameters under current RPC semantics, because that
  filter is a wildcard

Bounds:

```rust
// by_owner
(owner, cursor.unwrap_or(ObjectID::ZERO)) ..= (owner, ObjectID::MAX)

// by_owner_type
(owner, type_filter.clone(), cursor.unwrap_or(ObjectID::ZERO))
    ..= (owner, type_filter.clone(), ObjectID::MAX)
```

Keep cursor behavior inclusive.

## Tests To Update/Add

The behavior tests in `tests/store_execution.rs` exercise the owned-object index through
`DataStore` and must keep passing through the refactor — they are the regression net, not
candidates for rewrite. Only their internals (how they construct or inspect the index) change, not
their assertions. In particular:

- `test_owned_objects_tracks_address_owner_transfers`,
  `test_owned_objects_tracks_consensus_address_owner_writes`,
  `test_owned_objects_removes_non_address_owned_transitions`
- `test_local_deletion_removes_current_object_but_preserves_historical_lookup`,
  `test_local_wrap_removes_current_object_but_preserves_historical_lookup`,
  `test_unwrapped_write_clears_wrapped_latest_and_reindexes_owner`,
  `test_terminal_deleted_latest_prevents_reindexing_written_object`
- `test_rpc_owned_objects_iter_filters_and_pages_by_object_id`,
  `test_cloned_store_shares_owned_object_snapshot_guard`

The transfer, deletion, and wrap tests directly cover the "collect old object state" path; confirm
they assert against the owned-object query result (not just `get_object`) so they catch a stale
index row. If any of them only checks `get_object`, strengthen it to also assert the object is
absent from / present in the owner listing.

Remove file-index tests and add DB-index/store tests:

- `OwnedObjectIndexStore::owned_object_index_exists()` unwraps or propagates `anyhow::Result<bool>`
- seed construction uses `replace_from_objects()`
- store tests materialize owned-object results through `DataStore` or scan DB rows directly

Add focused tests:

- unfiltered owner scans return only that owner's objects in object ID order
- cursor behavior is inclusive
- exact type filters use the typed index and preserve object ID pagination
- wildcard type filters still match all type-parameter instantiations
- owner transfer deletes the old owner key and writes the new owner key
- type change deletes the old typed key and writes the new typed key
- deletion, wrapping, and unwrapped-then-deleted remove old keys
- initialized-empty metadata survives restart and does not trigger reinitialization
- unsupported metadata version returns an error

Add a concurrency/locking test if it is stable on supported platforms:

1. Open an `OwnedObjectIndexStore` for a temp root.
2. Attempt to open a second one for the same root.
3. Assert the second open fails with context indicating the index directory is already in use.

If this is platform-specific or flaky in CI, cover the error context in a unit test around the DB
open helper instead.

## Verification

Run focused tests first:

```bash
cargo test -p sui-fork owned_object
cargo test -p sui-fork store_execution::test_rpc_owned_objects_iter_filters_and_pages_by_object_id
```

Then run crate checks:

```bash
cargo test -p sui-fork
cargo xclippy -D warnings
```

Use at least a 10 minute timeout for cargo commands in this repository.
