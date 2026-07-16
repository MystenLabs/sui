<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Design: migrate `sui-fork` storage from filesystem to `sui-rpc-store`

> Supersedes `sui-rpc-store.md` (which was index-first). This is the working draft;
> we iterate on it before implementing.

## Context

`sui-fork` is a developer tool that forks a live Sui network at `forked_at_checkpoint`,
serves the `sui-rpc-api` trait stack, and executes new transactions locally via
`simulacrum`. Today all persistence is a hand-rolled filesystem cache
(`crates/sui-fork/src/filesystem.rs`): object BCS under `objects/<id>/<version>`, a
`latest` marker per object encoding live/deleted/wrapped, checkpoint/tx/effects/events
files, a tx→checkpoint map, and a whole-file owned-object index under `indices/`.

`sui-rpc-store` already defines RocksDB column families, encoders, and a full reader
(`RpcStoreReader`) for exactly these types, plus every RPC index we will eventually want.
Reusing it removes the bespoke filesystem store and aligns the fork's on-disk semantics
(tombstones, historical versions, indexes) with the real node. This first step targets the
**raw data** the fork needs to answer basic RPC: **objects, checkpoints, transactions**
(+ effects/events). Indexes (owner/type/balance/dynamic-field/coin/package) come later.

### Decisions already taken

1. **Ingestion driver = the sui-rpc-store `Indexer` orchestrator.** Feed locally-sealed checkpoints
   to the framework `Indexer` (`Indexer::from_store` + `add_pipelines`) via a custom
   `IngestionClient` that serves the fork's checkpoints; the pipelines derive and commit every CF
   row. `sui-consistent-store` is the engine those pipelines commit through — it is not a separate
   choice: `RpcStoreSchema` *is* a consistent_store schema, so any use of sui-rpc-store uses it.
   Read-after-execute is made airtight with a cheap barrier (execute awaits committer watermark ≥
   the sealed checkpoint), which is trivial because the fork produces exactly one checkpoint per
   transaction, sequentially.
2. **Writer facade lives in `sui-fork`** as a new `ForkRpcStore` (`crates/sui-fork/src/rpc_store.rs`),
   built on the *already-public* `schema::*::store()` / `Key` / `Objects::restore` helpers. No
   new public writer API in `sui-rpc-store` for now.
3. **Historical transactions get their real global `tx_seq`.** When materializing a pre-fork tx,
   also resolve its containing checkpoint so the true `tx_seq` is computed and the full
   tx-keyed CFs are populated; all tx reads then flow through `RpcStoreReader`.

## Goals (Phase 1)

- Replace filesystem object storage with `sui-rpc-store::objects` (versioned rows + tombstones).
- Replace filesystem checkpoint/tx/effects/events storage with the corresponding CFs.
- Keep the fork's defining behavior: **read local-first; on miss, fetch from GraphQL at the
  fork checkpoint; materialize into `sui-rpc-store`; never let remote fallback resurrect a
  locally removed object or overwrite a newer local write.**
- Keep `DataStore` as the facade implementing the `sui-rpc-api`/`SimulatorStore` trait stack.

## Non-Goals (Phase 1)

- No RPC indexes yet (`owned_objects_iter`, `dynamic_field_iter`, `get_balance`,
  `get_coin_info`, `package_versions_iter` keep returning their current `todo!`/empty).
  The owned-object index stays on its current path until Phase 2.
- No full chain backfill; sparse on-demand materialization only.
- No continuous background chain-tailing indexer.
- No migration of existing fork data dirs (dev tool → require a fresh `--data-dir`).

## What `sui-rpc-store` gives us (grounding)

- **Open**: `sui_consistent_store::Db::open::<RpcStoreSchema>(path, DbOptions) -> (Db, RpcStoreSchema)`
  (synchronous; used in both async and `#[test]` code). Use `default_rocksdb_config()`.
- **Write a row** (what every pipeline `commit` does): `db.batch().put(&schema.<cf>, &Key, &Value)`;
  values come from public per-CF `store()` helpers, e.g.
  - `objects::store(&Object)` / `objects::tombstone(TombstoneKind::{Deleted,Wrapped})`, key `objects::Key{id,version}`
  - `transactions::store(&tx, &sigs)`, key `U64Be(tx_seq)`
  - `checkpoint_summary::store(..)`, `checkpoint_contents::store(..)`, `effects::store(..)`, `events::store(..)`, etc.
- **Reuse derivation**: each pipeline is `Processor::process(&Arc<Checkpoint>) -> Vec<Row>` +
  `sequential::Handler::{batch,commit}` (`crates/sui-rpc-store/src/indexer/{objects,transactions,...}.rs`).
  `process()` already computes tombstones, `tx_seq` (`indexer::tx_seq_at`/`first_tx_seq`), output objects, etc.
- **Orchestrated ingestion**: `Indexer::from_store(store, ..)` + `add_pipelines(PipelineLayer, ..)`
  runs the enabled pipelines against a shared `Store<RpcStoreSchema>`; it pulls checkpoints from an
  `IngestionClient` and drives each pipeline's `process` → `commit`
  (`crates/sui-rpc-store/src/indexer/mod.rs`). Every commit is a `sui_consistent_store::Connection`
  write (the pipeline `commit` signature) — consistent_store is the storage engine underneath, not a
  separate store.
- **Objects bulk seed**: `Objects::restore(&schema, &Object, &mut Batch)` writes one `(id,version)`
  row; `restore_indexes(..)` drives a full `RestoreSource` stream (needed only once indexes arrive).
- **Read**: `RpcStoreReader::new(db, Arc<schema>)` implements `ObjectStore`/`ReadStore`/
  `RpcStateReader`/`RpcIndexes`. Direct schema getters:
  - `schema.get_object(id)` → latest live (reverse prefix scan, collapses tombstone→None)
  - `schema.get_object_by_key(id, v)` → exact live version
  - `schema.get_object_status_by_key(id, v)` → `Live | Tombstone(kind)` | missing (three-way)

### Two small helpers we must add (gaps)

1. **Latest three-state status** — reads need to tell *removed* (do not resurrect from GraphQL)
   from *unknown* (do fetch). `get_object(id)` collapses both to `None`. Add
   `RpcStoreSchema::get_object_status(id) -> Option<Status>` (reverse-prefix scan returning the
   top row's `Status`), mirroring `get_object` in `schema/objects.rs`. Prefer adding it upstream
   next to `get_object`.
2. **Bounded `<= version` read** — replaces `FilesystemStore::get_object_lt_or_eq_version` for
   child-object resolution. Add `RpcStoreSchema::get_object_lt_or_eq_version(id, bound) -> Option<Status>`
   via a reverse-prefix scan seeded at `objects::Key{id, bound}`. Also upstream.

Both are pure reads over the existing `objects` CF; no schema/proto change.

## Target architecture

```
sui-rpc-api / simulacrum            (unchanged trait consumers)
        |
        v
DataStore (facade, crates/sui-fork/src/store.rs)   -- keeps GraphQL fallback + tombstone rules
        |
        +-- GraphQLClient                          (unchanged; on-demand pre-fork fetch)
        |
        +-- ForkRpcStore (crates/sui-fork/src/rpc_store.rs)   NEW
                 |-- Db + RpcStoreSchema            (shared: Indexer writes, reader reads)
                 |-- Indexer (from_store) + ForkIngestionClient   -> ingests local checkpoints
                 |-- RpcStoreReader                 (all local reads)
                 |-- await_committed(seq)           -> read-after-execute barrier
                 |-- materialize_object / _historical_object / _transaction / _checkpoint  (backfill)
```

`DataStoreInner` swaps `local: FilesystemStore` for `rpc: ForkRpcStore` (owned-object index and
`local_snapshot_lock` stay until Phase 2 removes them). `DataStore` keeps implementing the trait
stack; each method delegates to `RpcStoreReader`/schema getters instead of `FilesystemStore`, with
the same GraphQL-fallback wrappers it has today.

## Two write paths

Local execution and pre-fork backfill are fundamentally different and stay separate.

### A. Local checkpoint ingestion (the sui-rpc-store `Indexer` — decision #1)

Locally executed transactions each produce exactly one checkpoint, sequentially (no parallel
execution). We ingest these through the sui-rpc-store `Indexer` orchestrator — the intended,
least-custom path — rather than hand-driving handlers:

- **Build the `Indexer` once at startup**: `Indexer::from_store(store, ..)` + `add_pipelines(layer, ..)`
  with only the Phase-1 raw-data pipelines enabled (`checkpoint_summary`, `checkpoint_contents`,
  `checkpoint_seq_by_digest`, `transactions`, `tx_seq_by_digest`, `tx_metadata_by_seq`, `effects`,
  `events`, `objects`). Seed each pipeline's watermark to `forked_at_checkpoint` so tip indexing
  begins at `forked_at_checkpoint + 1` (same technique as `restore::floor_unrestored_pipelines`).
- **Feed it a `ForkIngestionClient`** (custom `IngestionClientTrait`): `latest_checkpoint_number()`
  returns the highest checkpoint the fork has sealed; `checkpoint(n)` returns that sealed checkpoint
  as a `full_checkpoint_content::Checkpoint`; `chain_id()` returns the fork's chain id. The `Indexer`
  pulls and runs every enabled pipeline's `process` → `commit` for each new checkpoint; between
  transactions it idles.
- **Read-after-execute barrier.** The pipelines commit on the `Indexer`'s background task, so a read
  RPC issued right after an execute RPC can race the committer (an ordering gap, independent of how
  fast ingestion is). The execute path therefore awaits the committed watermark reaching the
  just-sealed checkpoint (`db.framework().watermarks`, min over enabled pipelines ≥ N) before
  returning. Cheap precisely because it is one checkpoint at a time, sequential — a sync point, not
  a throughput cost.
- **Assembling the checkpoint (main new write-side work).** simulacrum hands us execution pieces via
  `SimulatorStore::insert_*` (transaction, effects, events, written objects) plus a sealed
  summary/contents, *not* a full `Checkpoint`. `ForkIngestionClient::checkpoint(n)` must assemble a
  `full_checkpoint_content::Checkpoint` (summary, contents, per-tx `transaction`/`effects`/`events`,
  input + output `object_set`) from those pieces so the pipelines can derive their rows.
- **Consequences**: `tx_seq` is derived for free by `Transactions::process`
  (`network_total_transactions`); tombstones and version rows come from `Objects::process` (reads
  `effects.deleted/wrapped/unwrapped_then_deleted` at `lamport_version`), so the fork stops
  hand-encoding removal kinds and the old `apply_object_updates` + `removed_objects_from_effects`
  path is deleted.

The `Synchronizer` (`Store::install_sync`) is **not** used in Phase 1 (single sequential writer, raw
CFs only, no cross-CF index snapshot to coordinate). It is added in Phase 2 with the derived indexes,
which genuinely need cross-CF snapshot consistency.

### B. On-demand pre-fork backfill (direct/restore path)

GraphQL-fallback reads cache single items; these are historical rows below the fork point, not tip
progress, so they **do not advance watermarks** and are written directly (like `restore`):

- **Object (latest at fork ckpt)**: `db.batch().put(&schema.objects, &objects::Key{id, v}, &objects::store(&obj))`
  (equivalently `Objects::restore`). Sets no index rows in Phase 1.
- **Object (exact/bounded version)**: same, as a historical row; must not change what `get_object`
  reports as latest (it won't — reverse scan picks the true max).
- **Transaction (by digest, decision #3)**: fetch tx+effects+events *and* its containing checkpoint
  (`info.checkpoint` from GraphQL → fetch that checkpoint's summary+contents), compute
  `tx_seq = network_total_transactions - len + index_in_contents`, then write `transactions[tx_seq]`,
  `tx_seq_by_digest[digest]=tx_seq`, `tx_metadata_by_seq[tx_seq]`, `effects`, `events`. Guard: drop
  anything with `info.checkpoint > forked_at_checkpoint` (current pre-fork guard).
- **Checkpoint (by seq ≤ fork)**: write `checkpoint_summary`, `checkpoint_contents`,
  `checkpoint_seq_by_digest`.

Overlay safety (unchanged intent, now expressed via tombstones): before a backfill write,
consult `get_object_status(id)` — if the latest row is a tombstone or a newer version, skip the
`live`/latest-affecting write. Exact historical version rows are always safe to insert.

## Read paths (DataStore delegation)

- `get_object(id)` (latest): `schema.get_object_status(id)` → `Live`⇒return; `Tombstone`⇒`None`
  (no GraphQL); missing⇒GraphQL `AtCheckpoint(fork)` then materialize (path B).
- `get_object_at_version(id, v)`: `get_object_by_key`; miss⇒GraphQL `VersionAtCheckpoint`; store historical.
- `get_object_lt_or_eq_version(id, bound)` (child objects): new bounded helper;
  miss⇒GraphQL `RootVersion(bound)`; store historical.
- `get_transaction*`/`get_transaction_checkpoint`: `tx_seq_by_digest`→`transactions`/`effects`/
  `events`/`tx_metadata_by_seq`; miss⇒backfill (path B).
- `get_checkpoint*`: `checkpoint_summary`/`checkpoint_contents`/`checkpoint_seq_by_digest`;
  miss + seq ≤ fork ⇒ backfill.
- Highest checkpoint / `get_highest_indexed_checkpoint_seq_number`: keep DataStore's override
  (local watermark), independent of pipeline watermarks.

## Store layout / restart / migration

```
{data_dir}/
  seed_manifest.json          # sidecar, unchanged for now
  rpc_store/                  # Db::open::<RpcStoreSchema>
```

- **Restart**: reopen `rpc_store/`; validate chain-id / `forked_at_checkpoint` against CLI +
  `seed_manifest.json` (framework `chain_ids` CF gives us chain pinning for free). Local watermark
  = highest committed checkpoint; execution resumes on top.
- **Migration**: none. Switching store format requires a fresh `--data-dir`; document it.

## Files to change

- NEW `crates/sui-fork/src/rpc_store.rs` — `ForkRpcStore` (open Db+schema, build `Indexer` +
  `ForkIngestionClient`, `RpcStoreReader`, `await_committed`, `materialize_*` backfill, overlay guards).
- `crates/sui-fork/src/store.rs` — swap `FilesystemStore` → `ForkRpcStore` in `DataStoreInner`;
  rewrite read methods to delegate; the `SimulatorStore::insert_*` path now assembles the sealed
  checkpoint and lets the `Indexer` ingest it; drop `apply_object_updates` /
  `removed_objects_from_effects` / `RemovedObject*`.
- `crates/sui-fork/Cargo.toml` — add `sui-rpc-store`, `sui-consistent-store`,
  `sui-indexer-alt-framework`.
- `crates/sui-fork/src/lib.rs` — export the new module.
- `crates/sui-rpc-store/src/schema/objects.rs` — add `get_object_status` + `get_object_lt_or_eq_version`.
- `crates/sui-fork/src/filesystem.rs` — keep only what Phase 1 still needs (owned-object index +
  seed manifest); delete object/checkpoint/tx/effects/events file storage once `rpc_store/` is live.

## Phasing

1. **Scaffold** `ForkRpcStore`: open `rpc_store/`, `RpcStoreReader`, the two schema helpers.
2. **Objects**: route object reads/writes through `ForkRpcStore` (path A objects handler + path B
   materialize); enforce tombstone overlay. Remove filesystem object storage.
3. **Checkpoints + transactions/effects/events**: path A handlers + path B backfill with real `tx_seq`.
   Remove filesystem checkpoint/tx storage.
4. (**Phase 2, later**) Indexes via `install_sync` + index pipelines / `restore_indexes`; migrate
   the owned-object index; remove `filesystem.rs` and `local_snapshot_lock` entirely.

## Test plan

- Latest read: GraphQL fetched once, then served from `rpc_store`; second read makes no remote call.
- Exact-version and bounded (`lt_or_eq`) reads store historical rows without moving the latest pointer.
- Local delete/wrap writes a tombstone (via `Objects::process`); subsequent latest read returns
  `None` and does **not** hit GraphQL; historical version still readable.
- Unwrap/rewrite adds a newer live row that supersedes the tombstone.
- Local checkpoint ingestion is atomic and immediately visible (read-after-execute) with inline commit.
- Historical tx-by-digest backfill computes the correct `tx_seq` and round-trips through `RpcStoreReader`.
- Restart reopens `rpc_store/`, validates fork metadata, resumes execution.
- Old-format data dir fails with a clear "fresh --data-dir required" error.
- Existing `store_execution.rs` / `store_checkpoint_persistence.rs` / `store_transaction_fallback.rs`
  behavior tests keep passing (adapt internals, not assertions).

## Verification

- `cargo nextest run -p sui-fork` (raise timeout ≥ 10 min).
- `cargo nextest run -p sui-rpc-store` for the two new schema helpers.
- Manual smoke: fork testnet at a checkpoint, `get_object` a pre-fork object (fetch+persist),
  execute a PTB, immediately `get_object`/`get_transaction` the result from `rpc_store`, restart,
  confirm state persists. Then `cargo xclippy -D warnings` in `crates/sui-fork`.

## Open questions to resolve while implementing

- Shape of `ForkIngestionClient`: pull (`IngestionClientTrait`) vs streaming
  (`CheckpointStreamingClient`), and how newly-sealed checkpoints are surfaced to the `Indexer`'s
  poll loop (buffer/notify).
- Where the full `Checkpoint` is assembled from simulacrum's execution outputs — does simulacrum
  already expose a `CheckpointData`/`full_checkpoint_content::Checkpoint` builder we can reuse?
- Exact watermark-seeding at `forked_at_checkpoint` so the `Indexer` resumes at the right tip.
- Does `Db::open` need a Tokio runtime at open time (metrics tasks), or is it fully sync like the
  tests suggest? Confirm before wiring `DataStore::new`.
- Confirm public visibility of every `schema::<cf>::store()` helper + `Key` we need (objects,
  transactions, effects, events, checkpoint_summary/contents/seq_by_digest, tx_seq_by_digest,
  tx_metadata_by_seq); add `pub` upstream where missing.
- Exact `set_committer_watermark` value per handler when driving inline (checkpoint seq, tx_hi,
  epoch, timestamp) — mirror what the sequential committer passes.
- Whether Phase 1 should take a `Db` snapshot per checkpoint (`take_snapshot`) or defer all
  `at_snapshot` support to Phase 2 with the synchronizer.
