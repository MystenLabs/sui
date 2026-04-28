<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Owned Object Index Design

This document describes the first-version owned-object index design for
`sui-forking`. The design is intentionally small: it supports owned object
enumeration for locally materialized post-fork state while preserving the
existing object-version cache.

## Goals

- Keep `objects/<object_id>/latest` as the highest cached object version.
- Prevent locally deleted objects from being resurrected by remote fallback.
- Track live `Owner::AddressOwner` objects created or changed by local
  execution.
- Support `SimulatorStore::owned_objects(owner)` and the v2 RPC
  `list_owned_objects` path through `RpcIndexes::owned_objects_iter`.

This does not build a full pre-fork address inventory. Objects only enter the
owned-object index when local execution writes them through `update_objects`.

## Filesystem State

The object cache keeps the existing layout:

```text
objects/
  <object_id>/
    latest
    <version>
```

`latest` remains a cache marker for the highest object version written to disk.
It is not deleted when local execution deletes an object, because exact-version
reads must still be able to load historical versions.

Local execution can now add:

```text
objects/
  <object_id>/
    deleted
```

The `deleted` file is an explicit local tombstone. When present, current-object
reads return `None` and do not fall back to the remote GraphQL endpoint.
Exact-version reads ignore this marker and can still load
`objects/<object_id>/<version>`.

The owned-object index is stored separately:

```text
indices/
  owned_objects
```

`indices/owned_objects` is a BCS-encoded `Vec<OwnedObjectEntry>` sorted by
`object_id`:

```rust
struct OwnedObjectEntry {
    owner: SuiAddress,
    object_id: ObjectID,
    version: SequenceNumber,
    object_type: StructTag,
    balance: Option<u64>,
}
```

Only address-owned Move objects with a `StructTag` are indexed. Shared,
immutable, object-owned, consensus-owned, and package objects are excluded.

## Write Path

Remote object cache writes only call `write_object`. They write version files
and update `latest`; they do not clear `deleted` markers or update the owned
index.

Local execution flows through `DataStore::update_objects`:

1. For each deleted object ID:
   - write `objects/<object_id>/deleted`
   - remove the object ID from `indices/owned_objects`
2. For each written object:
   - write `objects/<object_id>/<version>`
   - update `objects/<object_id>/latest`
   - clear `objects/<object_id>/deleted`
   - upsert the owned index entry if the object is `Owner::AddressOwner`
   - remove any existing owned index entry otherwise
3. Rewrite `indices/owned_objects` via temp-file then rename.

The index file is deliberately simple: every local execution update reads the
current vector, applies removals/upserts with binary search by `object_id`, and
writes the sorted vector back.

## Read Path

`DataStore::get_object` checks the local deleted marker before reading from disk
or falling back to GraphQL. This keeps post-fork deletes authoritative.

`DataStore::get_object_at_version` remains exact-version only. It can return
historical cached object versions even if the current object is marked deleted.

`SimulatorStore::owned_objects(owner)` reads `indices/owned_objects`, filters by
owner, validates each entry against the current object state, then returns the
matching objects.

`RpcStateReader::indexes()` returns the `DataStore` itself as a minimal
`RpcIndexes` implementation. `owned_objects_iter` supports:

- owner filtering
- optional type filtering, with empty type parameters matching all type params
  for the same address/module/name
- cursor lower-bound filtering by `object_id`

Unsupported index methods return empty iterators or `None`.

## Guarantees And Limitations

- The current-object read path cannot resurrect locally deleted objects from the
  remote endpoint.
- The owned-object index is durable and sorted, but it is not a complete live
  object database.
- The index only covers locally materialized post-fork writes.
- Aggregate balance APIs are out of scope for this first version.
- Crash recovery is limited to atomic replacement of the index file. A future
  version can rebuild the index by scanning object files and deletion markers if
  stronger recovery is needed.

## Test Coverage

The design should be validated with tests for:

- deleted markers blocking latest/current reads while preserving exact-version
  reads
- owned index upsert, removal, persistence, and sort order
- address-owned writes appearing in `owned_objects`
- transfers moving entries between owners
- non-address-owned transitions removing entries
- local deletion removing entries and blocking remote resurrection
- RPC owned-object iteration type filtering and cursor behavior
