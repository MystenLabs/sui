<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` storage architecture

This document describes the storage architecture **as implemented** after the
migration from the filesystem cache onto stock `sui-rpc-store`.

## Constraints and goals

- **No upstream changes.** `sui-rpc-store` and `sui-rpc-node` are untouched;
  everything fork-specific lives in this crate. The fork serves RPC directly
  through `sui-rpc-api`'s `RpcService` (it does not use `sui-rpc-node`).
- **Reuse the stock indexes.** Local checkpoints are ingested by an embedded
  `sui-rpc-store` `Indexer` running `PipelineLayer::all()` (objects, owner,
  type, balance, package-versions, transactions, bitmaps, ...), so the fork
  gets the full derived-index surface without maintaining its own.
- **Pre-fork state is sparse.** The forked-from chain is only materialized on
  demand (or by seeding) from GraphQL, pinned at the fork checkpoint.

## Component map

```
gRPC clients ──► RpcService (sui-rpc-api)
                   │ reads                        │ writes
                   ▼                              ▼
             ForkRpcReader              ForkedTransactionExecutor
             /            \                       │
   RpcStoreReader      ForkStore ◄────── Context (publication lock,
   (stock, direct)      │                      indexer gating)
                        │                         │
                        │                     Simulacrum
                        │              (SimulatorStore = ForkStore)
      ┌─────────────────┼──────────────────┐
      │ remote:         │ inventory:       │ pending:
      │ RemoteSource    │ Inventory-       │ PendingCheckpoint-
      │ (GraphQL,       │ Initializer      │ Buffer (in-memory
      │  fork-pinned)   │ (lazy scans +    │  staging until seal)
      │                 │  markers)        │
      └────────┬────────┴──────────────────┘
               ▼
         LocalStore ───── LiveState (own RocksDB:
               │             ObjectID → Live(v) | Removed{v, kind})
               ▼
        rpc-store Db (RocksDB, stock RpcStoreSchema)
               ▲
            Indexer (17 pipelines + checkpoint broadcast)
               ▲
      SimulacrumIngestion — pulls each sealed checkpoint
      back OUT of the same rpc-store rows
```

Key roles:

- **`ForkRuntime`** (`runtime.rs`) owns the rpc-store `Db` + schema, the
  fork-owned `LiveState`, `fork_metadata.json` validation, and the embedded
  indexer (started via `Context::new_with_runtime`; watched by
  `indexer_stopped()`).
- **`ForkStore`** (`store.rs`) is composition + orchestration: local-first
  reads with remote fallback and persistence, checkpoint sealing, and the
  `SimulatorStore` surface Simulacrum executes against. Its collaborators:
  - **`RemoteSource`** (`remote.rs`): every GraphQL round-trip and all
    remote-read policy — object queries pinned at the fork checkpoint,
    post-fork gates for checkpoints and transactions, response-reference
    validation, inventory scans.
  - **`InventoryInitializer`** (`inventory.rs`): lazy one-time full
    enumerations that backfill the owner/type indexes, with completion
    markers, serialized under the snapshot lock shared with local writes.
  - **`PendingCheckpointBuffer`** (`pending.rs`): in-memory staging for the
    in-flight checkpoint between Simulacrum's piecemeal inserts and the
    atomic seal.
- **`LocalStore`** (`local_store.rs`) is fork-aware row access to the
  rpc-store plus the `LiveState` pointer table: object materialization,
  checkpoint/transaction persistence, and `get_latest_object_status`.
- **`ForkRpcReader`** (`rpc/reader.rs`) implements the upstream RPC storage
  traits, routing each method by key semantics: **immutably-keyed reads**
  (exact object versions, checkpoint/transaction digests and sequence
  numbers) go stock-reader first with `ForkStore` on a miss — a cached row
  cannot be wrong, and the miss-path double point-get is accepted for the
  simpler layering. **Latest-semantics reads** (`get_object`) go through
  `ForkStore` only: the stock reverse scan assumes a complete version
  history, which the fork's sparse `objects` CF violates, so a bare cached
  historical row must never be served as current.

## Data-dir layout

```
{root}/
  fork_metadata.json        network + fork checkpoint + chain id (validated on open)
  seed_manifest.json        immutable seed record (exclusive create)
  inventory_metadata.json   completion markers for inventory scans (temp+rename)
  rpc_store/                stock sui-rpc-store RocksDB (RpcStoreSchema)
  live_state/               fork-owned RocksDB (single CF fork_live_state)
```

## `LiveState`: the current-version authority

`sui-rpc-store` has no column family keyed by `ObjectID` that answers
"what is this object's current version, and is it live or removed?".
`object_by_owner`/`object_by_type` record latest live versions but are keyed
by owner/type and only cover indexed objects; and the fork's `objects` CF is
**sparse** (historical versions are cached on demand), so a reverse scan
cannot distinguish *removed* from *not cached*. `LiveState` is the fork's
authority for latest-reads and remote-fallback decisions:

- `Live(version)` — read `objects[(id, version)]` locally, never fall back.
- `Removed { version, kind }` — authoritative tombstone, never fall back.
- absent — no local knowledge; fall back to remote.

Data storage ordering: rpc-store rows commit **before** the pointer update
(a reader can transiently miss the pointer, which degrades to "unknown");
within one `apply_checkpoint`, removals stage **before** writes so a
wrap-then-rewrite in the same result lands `Live`.

## Executing transactions and indexing

**Sync = canonical data.** Each locally executed transaction synchronously
writes its object version rows + tombstones and the `LiveState` pointers
(`apply_local_object_diff`); sealing a checkpoint synchronously writes the
checkpoint summary/contents and every transaction's data/effects/events
(`save_pending_checkpoint_contents`). These rows are required immediately:
the executor needs read-your-writes for its next inputs, and the embedded
indexer's `SimulacrumIngestion` reads each sealed checkpoint back out of
these very rows.

**Async = everything derived (post-fork).** Owner, type, package-version,
balance, and bitmap indexes for local checkpoints are written by the indexer
alone (`first_checkpoint = forked_at + 1`). Checkpoint publication blocks on
`ForkRuntime::wait_for_indexed_checkpoint` (min watermark across all
pipelines), so by the time an execution returns, its checkpoint is fully
indexed. RPC reads issued afterwards always see complete derived state.
Subscribers receive checkpoints from the indexer's broadcast pipeline, so
their ordering is inherited from indexing.

**Pre-fork is the exception.** Pre-fork state never flows through the
indexer, so its derived rows are written synchronously: seed and inventory
saves (`save_indexed_live_object`) write owner/type/package/balance rows, and
lazy latest-object materialization (`save_live_object_if_current`) writes the
package-version row for fetched packages. These cover versions at or before
the fork point, ranges the indexer never touches, so every row still has
exactly one writer.

**Failures handling.** The `SimulatorStore` write surface cannot return
errors; a failed persist panics rather than letting execution continue on a
state that diverges from disk. An indexer stoppage is surfaced immediately by
the `indexer_stopped()` watchdog branch in `startup::run`, not as a delayed
publication timeout.

## Seeding and inventories

An *inventory* is a one-time complete remote enumeration (per address-owner,
object-owner, or type) at the fork checkpoint that backfills the stock index
CFs and records a completion marker in `inventory_metadata.json`; later
owner-scoped reads serve from the local index. Inventories run lazily on
first read.

Seeding (`--address`, `--object-id`) resolves an immutable manifest at
startup. An address seed performs the *same* complete scan as inventory
initialization, so the manifest records those addresses and
`save_seed_manifest_objects` marks their inventories complete after all
entries are saved — one scan, one marker. An address that owns nothing at
the fork checkpoint is authoritatively empty and is marked too. Explicit
object-id seeds never mark their owners (not a complete scan). Manifests
written before the `addresses` field existed fall back to lazy
initialization.

## Crash consistency and known gaps

- The pending checkpoint buffer is memory-only: a crash mid-publication
  loses the unsealed checkpoint/transactions while their object rows and
  live pointers persist. There is **no startup reconciliation** yet between
  `live_state` and the highest sealed checkpoint; this is the main known
  gap (fail-open risk: a pointer-less locally-written object could be
  re-resolved from pre-fork GraphQL after a crash in the tiny
  commit-to-pointer window).
- rpc-store and `live_state` are separate RocksDB instances; each commit is
  atomic internally but not across the two. Ordering makes removals
  fail-safe; see above for the write window.
- Address balances held in the accumulator (as opposed to coin objects) are
  not yet seeded or served; the balance index only reflects coin objects
  materialized pre-fork plus indexer-derived post-fork state.
- `simulate_transaction` is stubbed (no Simulacrum entrypoint yet).
- **Bounded child reads can serve stale history** (known, unfixed):
  `get_object_lt_or_eq_version` trusts the highest *local* row at or below
  the bound, but a sparse cache polluted by an exact-*historical*-version
  read (e.g. an RPC client fetching an old dynamic-field version) can hold a
  lower row than the true highest-≤-bound, which then wins without
  consulting the remote. Affects `read_child_object` on both the RPC and
  executor paths. Fix direction: short-circuit only on live-state authority
  or an authoritative tombstone; otherwise merge the remote
  `RootVersion(bound)` result with the local candidate by max version.
