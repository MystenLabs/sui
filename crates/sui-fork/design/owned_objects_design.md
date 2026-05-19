<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Owned Object Index Design

This document describes the first-version owned-object index design for
`sui-fork`. The design is intentionally small: it supports owned object
enumeration for seeded fork-point objects and locally materialized post-fork
state while preserving the existing object-version cache.

## Goals

- Keep live `objects/<object_id>/latest` files numeric-only while encoding local
  removed current state as version-first metadata.
- Prevent locally deleted objects from being resurrected by remote fallback.
- Track live `Owner::AddressOwner` objects created or changed by local
  execution.
- Support `SimulatorStore::owned_objects(owner)` and the v2 RPC
  `list_owned_objects` path through `RpcIndexes::owned_objects_iter`.

This does not build a full pre-fork address inventory by default. Seeded
objects enter the owned-object index when it is lazily initialized from
`seed_manifest.json`; post-fork objects enter when local execution writes them
through `update_objects`.

## Filesystem State

The object cache keeps the existing layout:

```text
objects/
  <object_id>/
    latest
    <version>
```

`latest` is a human-readable current-state marker. Live objects keep the
existing numeric-only format, `<version>`. Local execution encodes removals in
the same file as `<version>,deleted` or `<version>,wrapped`. When `latest`
contains deleted or wrapped state, current-object reads return `None` and do
not fall back to the remote GraphQL endpoint. Exact-version reads ignore this
state and can still load `objects/<object_id>/<version>`.

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
    object_ref: ObjectRef,
    object_type: StructTag,
    balance: Option<u64>,
}
```

Objects with `AddressOwner` and `ConsensusAddressOwner` types are kept in the
index. Shared, immutable, object-owned, and package objects are excluded. Coin
objects store their balance in the index; non-coin objects use `None`.

## Write Path

Remote object cache writes only call `write_object`. They write version files
and update numeric live `latest` metadata; they preserve existing deleted or
wrapped `latest` state and do not update the owned index.

Local execution flows through `DataStore::update_objects`:

Before applying the local diff, initialize `indices/owned_objects` from
`seed_manifest.json` if the index is missing.

1. For each deleted object ID:
   - write `objects/<object_id>/latest` as `<version>,deleted`
   - remove the object ID from `indices/owned_objects`
   - reject direct deletion if the current local latest state is `wrapped`; the
     effects path must report `unwrapped_then_deleted` for an unwrap-and-delete
     transaction
2. For each written object:
   - write `objects/<object_id>/<version>`
   - update `objects/<object_id>/latest` to numeric-only live state unless the object is deleted
   - upsert the owned index entry if the object is `Owner::AddressOwner`
   - remove any existing owned index entry otherwise
3. Rewrite `indices/owned_objects` via temp-file then rename.

The index file is deliberately simple: every local execution update reads the
current vector, applies removals/upserts with binary search by `object_id`, and
writes the sorted vector back.

## Read Path

`DataStore::get_object` checks object `latest` state before reading from disk or
falling back to GraphQL. This keeps post-fork removals authoritative.

`DataStore::get_object_at_version` remains exact-version only. It can return
historical cached object versions even if the current object is marked removed.

`SimulatorStore::owned_objects(owner)` reads `indices/owned_objects`, filters by
owner, then loads each candidate object through the normal object read path and
returns the matching objects at the indexed versions.

`RpcStateReader::indexes()` returns the `DataStore` itself as a minimal
`RpcIndexes` implementation. `owned_objects_iter` supports:

- owner filtering
- optional type filtering, with empty type parameters matching all type params
  for the same address/module/name, using the indexed object type
- cursor lower-bound filtering by `object_id`

Unsupported index methods return empty iterators or `None`.

## Guarantees And Limitations

- The current-object read path cannot resurrect locally deleted objects from the
  remote endpoint.
- The owned-object index is durable and sorted, but it is not a complete live
  object database; it covers seeded objects plus local post-fork writes.
- Owned-object reads and local object/index updates are coordinated by an
  in-process snapshot guard shared by cloned `DataStore` handles. This does not
  provide cross-process locking for multiple processes using the same data
  directory.
- The index does not enumerate unseeded pre-fork objects.
- Aggregate balance APIs are out of scope for this first version.
- Crash recovery is limited to atomic replacement of the index file. A future
  version can rebuild the index by scanning object files and object `latest`
  metadata if stronger recovery is needed.

## Test Coverage

The design should be validated with tests for:

- object `latest` removal state blocking current reads while preserving exact-version
  reads
- owned index upsert, removal, persistence, and sort order
- address writes appearing in `owned_objects`
- transfers moving entries between owners
- transitions away from address ownership removing entries
- local deletion removing entries and blocking remote resurrection
- RPC owned-object iteration type filtering and cursor behavior
