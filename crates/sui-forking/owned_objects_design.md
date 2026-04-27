# Owned Objects and Balances for `sui-forking`

## Context

`crates/sui-forking` currently persists:

- object versions under `objects/<object_id>/<version>`
- a `latest` file per object directory
- transactions, effects, events, and checkpoints

This is enough for object-by-id reads, but it is not enough to support:

- `SimulatorStore::owned_objects`
- RPC `list_owned_objects`
- RPC `get_balance`
- RPC `list_balances`

The current implementation has one limitation:

`update_objects()` ignores deleted objects. New versions are written, but there is no persisted
representation of an object leaving the live set, so the store cannot reconstruct current ownership
or balances correctly.

This document proposes a design for owned objects and balances for data created locally after the
fork starts, while fitting the filesystem model that already exists today.

## Goals

- Support owned object enumeration for locally executed transactions after the fork starts.
- Support coin balance queries for locally executed transactions after the fork starts.
- Reuse the current object-version filesystem layout instead of introducing a new primary store.
- Keep object files as the source of truth and treat ownership/balance indexes as derived state.
- Make it possible to rebuild indexes from disk on startup.
- Match the ordering and query shape expected by RPC as closely as practical.

## Non-goals

- Full historical owned-object queries at arbitrary checkpoints.
- Full pre-fork address inventory enumeration without explicit seeding or local materialization.
- Replacing the current object cache layout.

## High-level approach

The approach is to treat owned objects and balances as derived indexes over the locally materialized
live object set.

The source of truth remains:

- `objects/<object_id>/<version>` for object contents
- per-object metadata identifying which version, if any, is currently live

The derived state is maintained in memory:

- an owner index for address-owned objects
- a balance index keyed by `(owner, coin_type)`

If the process restarts, the derived state is rebuilt by scanning the live object set on disk.

## Proposed filesystem changes

### Object layout

Keep:

- `objects/<object_id>/<version>`: BCS-encoded `Object`

Add:

- `objects/<object_id>/live`: text file containing the currently live version

Semantics:

- If `live` exists, the object is currently live at that version.
- If `live` does not exist, the object is not in the live set.
- Historical versions remain on disk even after deletion or wrapping.

### Why add `live` instead of reusing `latest`

`latest` today tracks the highest version written to disk (via `std::cmp::max`), which is correct
for resolving the current version of an object. However, it cannot represent an object leaving
the live set: deletions and wrapping need a durable way to say "this object has no live version",
and `latest` always points to some version.

We should either:

- replace `latest` with `live`, or
- keep `latest` for cache/debug purposes and introduce `live` for correctness

This design treats the `live` file as the authoritative liveness marker.

## In-memory indexes

The owner and balance indexes live inside `DataStore` as plain fields. `DataStore` derives `Clone`,
but the indexes do not need interior mutability or `Arc` wrapping — all access goes through
`Context`'s existing `Arc<RwLock<Simulacrum<OsRng, DataStore>>>`, which serializes reads and writes.
Cloning `DataStore` outside of that path is not expected; if it happens, the clone gets a snapshot
of the indexes at that point in time.

### Owner index

Maintain an in-memory `BTreeMap<OwnerIndexKey, SequenceNumber>`.

Suggested key:

```rust
struct OwnerIndexKey {
    owner: SuiAddress,
    object_type: StructTag,
    inverted_balance: Option<u64>,
    object_id: ObjectID,
}
```

This follows the same shape as the RPC index in `sui-core`:

- owner first
- then type
- then descending balance for coin-like objects via `!balance`
- then object id as a stable tie-breaker

This is enough to support:

- `SimulatorStore::owned_objects(owner)`
- RPC `list_owned_objects`
- owner/type filtering
- stable cursor pagination

Only index:

- `Owner::AddressOwner`
- `Owner::ConsensusAddressOwner` normalized as address-owned

Do not index:

- `Owner::ObjectOwner`
- `Owner::Shared`
- `Owner::Immutable`

Packages should also be excluded from this index, because RPC owned-object pagination expects a
`StructTag`.

### Balance index

Maintain an in-memory `BTreeMap<(SuiAddress, StructTag), i128>`.

Use `i128` internally so updates can apply positive and negative deltas naturally.
Clamp to `u64` when returning RPC-facing `BalanceInfo`.

This is enough to support:

- RPC `get_balance`
- RPC `list_balances`

For `Coin<T>` objects:

- add the balance when a live address-owned coin enters the live set
- subtract the balance when a live address-owned coin leaves the live set

## What counts toward balances

### Phase 1

Support balance aggregation from address-owned `Coin<T>` objects only.

This is the smallest useful feature and aligns with the owned-object index.

### Phase 2

Optionally extend support to address-balance accumulator objects.

RPC v2 renders balances as:

- total indexed balance
- optionally split into coin balance and address balance

The current fullnode implementation derives address balance separately from accumulator objects.
If we want exact parity for protocols using address-balance gas payments, we should also track
those accumulator-backed values.

This can be added independently of the owner index.

## Write path for local execution

The core rule is:

- object files are written first
- live metadata is updated atomically via write-to-temp + rename
- in-memory indexes are updated from transaction effects

For every locally executed transaction, `update_objects()` should process both deleted and written
objects.

All `live` marker updates must be atomic: write the new contents to a temporary file in the same
directory, then `rename()` it to `live`. On POSIX systems `rename()` is atomic within a single
filesystem, so a crash can never leave a half-written `live` file. For deletions, `unlink()` the
`live` file directly — this is also atomic.

### Deleted / wrapped objects

For each `(object_id, old_version, _)` in `deleted_objects`:

1. Load the old live object from `objects/<id>/live`.
2. Remove the object's contribution from the owner index if it was address-owned.
3. Remove the object's contribution from the balance index if it was an address-owned coin.
4. Atomically remove `objects/<id>/live` (unlink).

This should also apply to wrapped objects if they appear in the deleted set exposed by effects.

### Written objects

For each written object:

1. If the object already has a live version, load that old live object and remove its old index
   contributions first.
2. Write `objects/<id>/<new_version>` (the BCS object file must be fully on disk before step 3).
3. Atomically update `objects/<id>/live` to `<new_version>` (write to a temp file, then rename).
4. Add the new owner's contribution to the owner index if the object is address-owned.
5. Add the new coin contribution to the balance index if the object is an address-owned `Coin<T>`.

This uniformly handles:

- create
- mutate
- transfer
- unwrap to address ownership
- change of object type
- change of coin balance

### Crash recovery

Because `live` updates are atomic, a crash at any point leaves the filesystem in one of two
consistent states:

- `live` still points to the old version (crash before rename): rebuild sees the old state. The
  new BCS file may exist on disk as an orphan, which is harmless.
- `live` points to the new version (crash after rename): rebuild sees the new state.

In-memory indexes are always rebuilt from disk on startup, so they are never stale after a crash.
No WAL is required.

## Rebuild on startup

On startup, rebuild the in-memory indexes by scanning the live object set:

1. Iterate `objects/*/live`.
2. Parse the live version for each object id.
3. Load `objects/<id>/<live_version>`.
4. Reconstruct:
   - owner index entries
   - balance index entries

This keeps the design simple:

- no separate on-disk owner table
- no separate on-disk balance table

The filesystem object store remains the only durable state we need. See "Crash recovery" above for
why no WAL is required.

## Tradeoffs

There are three realistic options for owned-object and balance support in `sui-forking`.

### Option A: filesystem-only live scans

Use the filesystem as both the source of truth and the query path. Each owned-object or balance
query scans the live object set on demand: iterate `objects/*/live`, load each referenced object,
inspect its owner and type, and sort/paginate the filtered results. No in-memory indexes are
maintained, so the write path only needs to update object files and `live` markers.

Pros:

- smallest implementation
- no secondary state to keep in sync in-process
- correctness comes entirely from object files and `live` markers

Cons:

- every owned-object query scans the full live set
- every balance query scans the full live set
- pagination requires repeated filtering and sorting work
- query latency grows with the number of live objects

This is a good fit if we want the smallest correctness-first implementation and expect the local
fork state to stay small.

### Option B: rebuildable in-memory indexes (recommended)

Pros:

- object files remain the source of truth
- owned-object queries become prefix scans instead of full-store scans
- balance queries become direct lookups or prefix scans
- ordering and cursor pagination align more naturally with RPC behavior
- indexes are disposable and can be rebuilt from disk on startup

Cons:

- more implementation work than filesystem-only scans
- local writes must update the derived indexes correctly
- restart requires a rebuild pass over the live set

This is the best tradeoff if we expect repeated RPC-style queries and want reasonable performance
without introducing a second durable store.

### Option C: durable on-disk secondary indexes

Pros:

- fast query path without a startup rebuild
- avoids repeated full-store scans

Cons:

- highest complexity
- requires keeping multiple durable structures in sync
- schema evolution becomes more expensive while the feature is still changing

This is likely premature for the first version. It only becomes attractive if startup rebuild time
or in-memory index size turns into a practical issue.

## Recommended direction

The recommended path is Option B:

- use the filesystem as the durable source of truth
- add `live` markers for correct liveness
- maintain rebuildable in-memory owner and balance indexes for query speed

If we want a smaller first milestone, we can land Option A first and treat Option B as a follow-up
optimization. The `live` marker change is useful in either case.

## Query behavior

### `SimulatorStore::owned_objects(owner)`

Implement this by:

1. seeking the owner prefix in the in-memory owner index
2. resolving each `(object_id, version)` to the object from disk
3. yielding `Object`

This avoids scanning all live objects on every query.

### RPC `list_owned_objects`

The in-memory owner index should be ordered to match RPC pagination requirements:

- owner prefix scan
- optional type filter
- descending coin balance ordering for coin-like objects
- stable cursor on `(owner, object_type, balance, object_id, version)`

The cursor can mirror `OwnedObjectInfo`.

### RPC `get_balance`

Look up `(owner, coin_type)` in the in-memory balance index and return the clamped result.

### RPC `list_balances`

Iterate the balance map over the `(owner, coin_type)` prefix.

## Pre-fork data and seeding

This design is intentionally scoped to local post-fork execution and locally materialized objects.

Pre-fork remote fetches (e.g. `get_latest_object` pulling from a remote fullnode) must NOT create
`live` markers. Remote fetches populate version files for use as execution inputs, but they do not
carry ownership information — the historical query path does not return owner or type metadata
needed for the owner and balance indexes. Only local transaction execution effects establish or
remove liveness, because effects are the authoritative source of ownership transitions.

Implications:

- pre-fork objects only enter the live set when they appear in the written set of a locally
  executed transaction's effects
- if an address has pre-fork owned objects that were never involved in a local transaction, they
  will not appear in owned-object enumeration
- the `latest` marker (used by the remote fetch cache path) remains independent of `live`

That is acceptable for `sui-forking`, because explicit seeding followed by local execution is the
mechanism for making the fork aware of external state.

## Historical queries

This design does not attempt to answer:

- "which objects did address A own at checkpoint C?"
- "what was address A's coin balance at checkpoint C?"

For that, we would need checkpoint-aware index deltas, for example:

- `indices/owner_deltas/<checkpoint>.bcs`
- `indices/balance_deltas/<checkpoint>.bcs`

or a compact checkpoint-stamped owner history table.

That should be treated as a separate phase. The live-index design proposed here is a good base for
that later work because all ownership and balance transitions will already be centralized in the
local write path.

## Suggested implementation order

1. Fix object liveness representation:
   - add `live` markers
   - stop treating `latest` as live state

2. Fix local state updates:
   - process `deleted_objects`
   - update live markers correctly

3. Choose the first query path:
   - Option A: implement filesystem-only scans for owned objects and balances
   - Option B: add rebuildable in-memory indexes immediately

4. If we choose Option B, add startup rebuild:
   - rebuild owner and balance indexes from live objects on disk

5. If we choose Option B, implement in-memory owner index:
   - back `SimulatorStore::owned_objects`
   - back RPC `list_owned_objects`

6. If we choose Option B, implement in-memory balance index:
   - back RPC `get_balance`
   - back RPC `list_balances`

7. Optionally add address-balance accumulator support.

## Open questions

1. Should `latest` be removed entirely and replaced by `live`, or should both coexist?
2. Do we want balance support to include address-balance accumulators in the first version?
3. Do we want rebuild-on-startup only, or should we also persist derived indexes for faster
   startup?
