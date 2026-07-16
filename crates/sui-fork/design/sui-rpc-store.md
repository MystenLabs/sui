<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Integrating `sui-rpc-store` with `sui-fork`

## Summary

`sui-fork` should stop maintaining its own object files and ad hoc secondary
indexes. Instead, it should use `sui-rpc-store` as the local physical store for
materialized objects, live-object state, owned-object indexes, dynamic-field
indexes, balance indexes, and related RPC indexes.

`sui-fork` still needs a fork-specific layer above `sui-rpc-store`. That layer
decides which remote objects must be fetched from GraphQL, which checkpoint they
must be fetched at, and whether the fetched object is just a historical object
row or part of a complete indexed inventory. `sui-rpc-store` should then handle
durable storage, tombstones, live pointers, and index maintenance.

The resulting split is:

```text
sui-rpc-api / simulacrum
        |
        v
DataStore in sui-fork
        |
        +-- ForkMaterializer: checkpoint-scoped GraphQL reads
        |
        +-- ForkRpcStore: thin writer/reader facade over sui-rpc-store
                |
                +-- objects, live_objects, object_by_owner, object_by_type
                +-- balance, package_versions, checkpoint/tx/event data over time
```

## Goals

- Move object BCS storage out of `objects/<id>/<version>` files and into
  `sui-rpc-store::schema::objects`.
- Move current object state out of `objects/<id>/latest` files and into
  `sui-rpc-store::schema::live_objects` plus object tombstones.
- Replace `indices/owned_objects` with `sui-rpc-store`'s
  `object_by_owner` index.
- Use `sui-rpc-store` indexes for balances, dynamic fields, object type scans,
  package versions, and coin metadata as those RPCs are enabled in `sui-fork`.
- Preserve `sui-fork`'s remote fallback behavior: missing pre-fork data is
  fetched from GraphQL at the fork checkpoint and materialized locally.
- Preserve local overlay behavior: post-fork writes and removals are
  authoritative and must never be overwritten by later remote materialization.
- Keep address-owned indexes seed-bounded. A sparse object cache must not be
  mistaken for complete pre-fork address ownership; only seed materialization
  plus local execution defines the tracked address-owner universe.

## Non-Goals

- Running a full `sui-rpc-store` indexer for the upstream chain.
- Backfilling the complete chain state at the fork checkpoint.
- Replacing all filesystem metadata in the first step. The seed manifest and
  fork metadata can remain sidecar files initially.
- Solving historical GraphQL availability limitations for every index. Address
  ownership uses explicit seeds; dynamic-field and type scans may still rely on
  checkpoint-scoped GraphQL enumeration where available.

## Current State

`sui-fork` currently has three local object/index mechanisms:

- `FilesystemStore` stores object versions as BCS files under `objects/`.
- `objects/<id>/latest` tracks the current live version, or local
  `deleted`/`wrapped` state.
- `indices/owned_objects` stores a whole-file BCS vector of
  `OwnedObjectEntry`.

This makes owned-object support possible, but it duplicates functionality that
already exists in `sui-rpc-store`:

- `objects` stores every live object version and tombstone rows.
- `live_objects` maps object ID to the latest live version.
- `object_by_owner` supports address-owner and object-owner scans. The
  object-owner side is the dynamic-field index.
- `object_by_type` supports type scans and coin metadata discovery.
- `balance` stores coin-object and accumulator-derived balance components.
- `RpcStoreReader` already implements `ObjectStore`, `ReadStore`,
  `RpcStateReader`, and `RpcIndexes`.

The missing piece is not schema coverage. The missing piece is a fork-aware
materialization and write API.

## Target Architecture

### `DataStore`

`DataStore` remains the main `sui-fork` facade that implements the trait stack
required by Simulacrum and `sui-rpc-api`.

Its job is orchestration:

- answer reads from `sui-rpc-store` when possible;
- fetch missing pre-fork data from GraphQL at the correct checkpoint;
- materialize fetched data through a `sui-rpc-store` writer;
- apply local execution diffs atomically to objects and indexes;
- prevent local tombstones and post-fork writes from being overwritten by
  remote fallback.

It should not know column-family details. Those details belong behind a small
writer facade in `sui-rpc-store`.

`RpcService` should still receive `DataStore` as the `RpcStateReader`, not a
bare `RpcStoreReader`, until every needed inventory is eagerly materialized.
`DataStore` must intercept object and index reads so it can perform GraphQL
fallback, hydrate missing inventories, and enforce local tombstones before
delegating to `RpcStoreReader`.

For execution, `DataStore` also keeps the fork-specific `ChildObjectResolver`
behavior. `RpcStoreReader`'s child resolver is intentionally read-only and
returns `None`; the fork needs bounded child-object reads backed by the
materializer and the object tombstone rules described below.

### `ForkMaterializer`

`ForkMaterializer` is a `sui-fork` layer that owns GraphQL policy. It should be
the only code that decides how to fetch pre-fork objects.

Responsibilities:

- fetch object ID at `forked_at_checkpoint` for current-object reads;
- fetch `(object_id, version)` at `forked_at_checkpoint` for exact historical
  reads;
- fetch root-version or bounded child-object reads without crossing the fork
  checkpoint;
- enumerate address-owned objects at the fork checkpoint when an owner
  inventory is being hydrated;
- enumerate dynamic fields for a parent at the fork checkpoint when a parent
  inventory is being hydrated;
- fetch balance-related objects or accumulator fields needed to hydrate balance
  indexes;
- validate fetched objects against seed refs when materializing a seed manifest.

GraphQL fetches must always be checkpoint-scoped. A current read from the fork
base should use `AtCheckpoint(forked_at_checkpoint)`. An exact-version read
should use `VersionAtCheckpoint { version, checkpoint: forked_at_checkpoint }`.
Bounded child-object reads should use the equivalent root-version query scoped
to the fork checkpoint.

### `ForkRpcStore`

`ForkRpcStore` is a narrow writer/reader facade over `sui-rpc-store`.

It should live either in `sui-rpc-store` as a public writer API or in
`sui-fork` as a very thin adapter. Prefer adding the writer API to
`sui-rpc-store` so schema coupling stays with the schema.

Core operations:

```rust
struct ForkRpcStore {
    reader: RpcStoreReader,
    writer: RpcStoreWriter,
}

impl ForkRpcStore {
    fn get_latest_object_state(id: ObjectID) -> Result<ObjectState>;
    fn get_object_at_version(id: ObjectID, version: SequenceNumber) -> Result<Option<Object>>;
    fn get_object_at_or_before(
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> Result<ObjectState>;

    fn materialize_historical_object(object: &Object) -> Result<()>;
    fn materialize_base_live_object(object: &Object, scope: IndexScope) -> Result<()>;
    fn apply_local_object_diff(diff: LocalObjectDiff) -> Result<()>;
}
```

`ObjectState` should distinguish:

- `Live(Object)`;
- `Removed { version, kind }`;
- `Missing`.

This distinction is required because `live_objects` missing means either
"unknown, try GraphQL" or "known removed, do not resurrect". The current
filesystem `latest` file encodes that distinction; the `sui-rpc-store`
replacement should expose it by reading the highest `objects` row for an object
and interpreting tombstones.

`IndexScope` should distinguish at least:

- `RawOnly`: write `objects` and possibly `live_objects`, but do not mark any
  remote inventory complete.
- `DynamicFieldInventory(parent)`: object belongs to a completed parent
  dynamic-field inventory.
- `Seed`: object came from the immutable seed manifest and may initialize the
  address-owned object and balance indexes.

The exact representation can be simpler, but the design needs some notion of
which sparse materializations are complete enough to back an RPC list response.

## Object Read Path

### Current object read

For `get_object(object_id)`:

1. Ask `ForkRpcStore::get_latest_object_state(object_id)`.
2. If `Live(object)`, return it.
3. If `Removed`, return `None`. Do not query GraphQL.
4. If `Missing`, fetch the object from GraphQL at
   `forked_at_checkpoint`.
5. If GraphQL returns an object, call `materialize_base_live_object`.
6. Re-read or return the fetched object.

`materialize_base_live_object` must be overlay-aware:

- It may insert the base `(id, version)` row if missing.
- It may set `live_objects` only if no newer local state exists.
- It must not replace a post-fork live pointer.
- It must not remove or overwrite a local tombstone.
- It must not add secondary index rows for an object whose current local state
  is already newer or removed.

### Exact-version read

For `get_object_at_version(object_id, version)`:

1. Read `(object_id, version)` from `sui-rpc-store::objects`.
2. If the row is live, return it.
3. If the row is a tombstone, return `None`.
4. If missing and `version` can belong to the remote fork base, query GraphQL
   with `VersionAtCheckpoint`.
5. Store the fetched object as a historical object row.

An exact historical materialization should not update `live_objects` or
secondary indexes unless the caller separately proves that version is also the
base live version.

### Bounded version read

For child-object reads and other `<= version` lookups:

1. Read the highest local `objects` row for the object at or below the bound.
2. If it is live, return it.
3. If it is a tombstone, return `None`.
4. If there is no local row, query GraphQL with the checkpoint-scoped
   root-version query.
5. Store the fetched row as historical unless it is also known to be the base
   live row.

This replaces the current directory scan in
`FilesystemStore::get_object_lt_or_eq_version`.

## Index Hydration

Object storage can be sparse. Address-owned index results are intentionally
seed-bounded rather than globally complete.

`sui-fork` should track completion only for remote inventories it still hydrates
on demand, such as object-owner dynamic-field scans and type scans used for coin
metadata. Address ownership must not use on-demand owner GraphQL scans.

### Owned objects

`owned_objects_iter(owner, type_filter, cursor)` should:

1. Delegate directly to `RpcStoreReader::owned_objects_iter`.
2. Return only objects materialized from the immutable seed manifest or written
   by local execution.
3. Return empty for unseeded owners with no local writes.

Startup must materialize every seed manifest object ref into `sui-rpc-store`,
validate the fetched object ref, and index address-owned coins into both
`object_by_owner` and `balance`. Re-running startup should be idempotent and
should not need any owner inventory completion marker.

Post-fork transfers are handled by local diffs. A transfer into an unseeded
owner makes that local object visible for the recipient, but does not imply any
knowledge of the recipient's pre-fork inventory.

### Dynamic fields

`dynamic_field_iter(parent, cursor)` should use the same pattern:

1. Ensure the parent dynamic-field inventory is hydrated.
2. Enumerate the parent's dynamic fields from GraphQL at the fork checkpoint.
3. Fetch each dynamic-field object by ID/ref at the fork checkpoint.
4. Materialize rows through `ForkRpcStore` using
   `DynamicFieldInventory(parent)`.
5. Delegate to `RpcStoreReader::dynamic_field_iter`.

`sui-rpc-store` already represents dynamic fields as `ObjectOwner(parent)` rows
in `object_by_owner`, so no new dynamic-field CF is needed.

### Balances

Balances have two inputs in `sui-rpc-store`:

- owned `Coin<T>` objects contribute the `coin` component;
- accumulator-root dynamic fields contribute the `address` component.

`get_balance(owner, coin_type)` and `balance_iter(owner, cursor)` should
delegate directly to `RpcStoreReader`. Address-owned `Coin<T>` balances are
materialized from seeded coin objects and local execution writes. Unseeded
owners with no local coin writes return no balance rows.

Accumulator-root dynamic-field objects should be fetched when GraphQL exposes
enough checkpoint-scoped metadata to find the relevant fields.

Until the accumulator half is implemented, the API should not claim complete
balance semantics unless the missing component is known to be irrelevant for
the requested protocol/version. A clear unsupported or incomplete-index error is
better than returning a partial balance as if it were complete.

### Coin info and package versions

`get_coin_info` depends on `object_by_type` rows for coin metadata,
treasury cap, and regulated metadata wrapper objects. Package-version listing
depends on `package_versions`.

These can be hydrated by:

- restoring package and metadata objects when they are fetched by ID;
- adding explicit GraphQL materializers for coin metadata and package version
  lookups;
- or deferring these RPCs until the relevant object/type inventories are
  materialized.

## Local Execution Write Path

Local execution must update `sui-rpc-store` atomically. The existing
`DataStore::apply_object_updates` behavior should become a single
`ForkRpcStore::apply_local_object_diff` call.

The writer needs:

- input objects before the write, for deleting old owner/type rows and
  computing negative balance deltas;
- output objects after the write, for inserting new rows;
- removed object IDs and removal kind (`deleted`, `wrapped`,
  `unwrapped_then_deleted`);
- the effects lamport version for tombstone rows;
- transaction/effects/events/checkpoint metadata if those are also being moved
  into `sui-rpc-store`.

For each local transaction or sealed local checkpoint:

1. Read all input object states from `sui-rpc-store` before mutating the store.
2. Stage `objects` rows for every written object.
3. Stage `objects` tombstones for deleted, wrapped, and unwrapped-then-deleted
   objects.
4. Delete `live_objects` rows for removed objects.
5. Put `live_objects` rows for written live objects.
6. Delete old `object_by_owner` and `object_by_type` rows derived from input
   objects.
7. Put new `object_by_owner`, `object_by_type`, and `package_versions` rows
   derived from output objects.
8. Merge balance deltas derived from old input objects and new output objects.
9. Commit the batch.

The object index logic should reuse the same helpers the existing
`sui-rpc-store` pipelines use:

- `objects::store` and `objects::tombstone`;
- `live_objects` keys and values;
- `object_by_owner::store`;
- `object_by_type::store`;
- `balance::delta` / `Balance::restore` semantics;
- `package_versions` restore helpers.

This keeps fork writes aligned with the normal indexer semantics.

## Store Layout

Under the existing fork root:

```text
{data_dir}/
  seed_manifest.json
  rpc_store/
    CURRENT
    MANIFEST-...
    ...
```

Initial integration can leave checkpoint, transaction, and seed metadata in the
existing filesystem files. Object files and `indices/owned_objects` should stop
being written once `rpc_store/` is active.

Longer term, transaction, effects, events, checkpoint summaries, and checkpoint
contents should also move into `sui-rpc-store`, because the schema and reader
adapters already support them.

## Metadata and Completeness

The design needs metadata for fork-specific materialization state. This can
start as a small sidecar file, but storing it in `sui-rpc-store` is cleaner once
the writer API exists.

Required metadata:

- store format version;
- forked network name and chain identifier;
- `forked_at_checkpoint`;
- completed dynamic-field parent inventories;
- completed type inventories used by coin-info hydration;
- seed-manifest digest or version, so stale seed state is not reused silently.

Address-owner completeness metadata is intentionally absent. Without explicit
seeds, a direct object read must not be interpreted as evidence that an owner
inventory is complete.

## Restart Behavior

On restart:

- Open `rpc_store/` first.
- Validate its fork metadata against CLI input and `seed_manifest.json`.
- Reuse completed inventory markers.
- Continue applying local transactions on top of the existing live state.
- Refuse to rebuild an index from seed data if local checkpoints have advanced
  and the store has no reliable metadata proving the rebuild is safe.

This preserves the current fail-closed behavior around stale
`seed_manifest.json` rebuilds.

## Migration Strategy

Because `sui-fork` is a developer tool, the first implementation can require a
new data directory when switching to `sui-rpc-store`.

If we want compatibility with existing data directories, add a one-time import:

1. Open old `objects/` and `indices/owned_objects`.
2. Write object versions to `sui-rpc-store::objects`.
3. Convert live `latest` markers to `live_objects` rows or tombstones.
4. Restore owner/type/balance/package indexes from live objects.
5. Write fork metadata marking the migration complete.
6. Leave old files untouched but ignored.

Do not mix active writes to both formats.

## Implementation Plan

1. Add a `sui-rpc-store` writer facade for sparse fork materialization.
   - Open `RpcStoreSchema` under `rpc_store/`.
   - Expose bounded object-status reads.
   - Expose live, historical, and tombstone writes.
   - Expose local object-diff application.

2. Wire `sui-fork::DataStore` object reads through `ForkRpcStore`.
   - Replace filesystem object reads/writes.
   - Keep GraphQL fallback in `sui-fork`.
   - Preserve tombstone-based remote fallback blocking.

3. Move owned-object indexes.
   - Hydrate seed manifests into `object_by_owner`.
   - Hydrate seed coin objects into `balance`.
   - Delegate `owned_objects_iter` to `RpcStoreReader`.
   - Remove `OwnedObjectEntry` and `indices/owned_objects`.

4. Add remote-inventory completion tracking.
   - Dynamic-field parent markers.
   - Type inventory markers used for coin info.
   - Fail closed when an index query cannot be hydrated.

5. Move dynamic fields and balances.
   - Implement checkpoint-scoped GraphQL enumeration for dynamic fields.
   - Restore object-owner rows for dynamic fields.
   - Restore coin-object balance rows.
   - Add accumulator-root balance hydration when supported.

6. Move raw transaction and checkpoint data, if desired.
   - Use `sui-rpc-store` transaction/effects/events/checkpoint CFs.
   - Keep `DataStore` as the GraphQL fallback facade for missing pre-fork
     history.

7. Delete obsolete filesystem object/index code.
   - Remove `objects/` object storage methods.
   - Remove `indices/owned_objects` methods and tests.
   - Keep only fork metadata files that intentionally remain outside
     `sui-rpc-store`.

## Test Plan

- Current object read fetches from GraphQL once and persists through
  `sui-rpc-store`.
- Exact-version reads store historical rows without changing the live pointer.
- Local deletion writes a tombstone and prevents remote resurrection.
- Local wrapping writes a wrapped tombstone and prevents current reads.
- Local unwrap/write clears wrapped state by writing a newer live row.
- Seed materialization indexes address-owned objects and balances and survives
  restart without remote owner hydration.
- Unseeded owner object and balance queries return empty without calling
  GraphQL.
- Transfers delete the old owner row and insert the new owner row.
- Coin transfers update both owned-object and balance indexes.
- Dynamic-field hydration enumerates object-owned children by parent.
- `get_object_at_or_before` returns a live row, tombstone miss, or remote
  fallback correctly.
- Restart reopens `rpc_store/` and validates fork metadata.
- Old-format data directories either fail with a clear error or complete the
  one-time migration.

## Open Questions

- Should generic object reads write secondary index rows immediately, or only
  write raw object/live rows until a complete inventory is hydrated?
- Should fork materialization metadata live in a `sui-rpc-store` CF from the
  start, or begin as a `sui-fork` sidecar file?
- Which GraphQL queries are available for checkpoint-scoped dynamic-field and
  accumulator-balance hydration at older checkpoints?
- Should local execution update `sui-rpc-store` per transaction, or should
  `sui-fork` assemble a full local checkpoint and reuse more of the existing
  `sui-rpc-store` pipeline processors?
- Do we want a one-time importer for existing filesystem data, or is a new data
  directory acceptable for the first cut?
