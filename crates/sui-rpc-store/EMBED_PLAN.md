# Embedding sui-rpc-store into the fullnode (working plan)

Living plan + progress tracker for the `embed-rpc-store` work. Survives
context clears and compactions. See `SUMMARY.md` for the full design of the
`sui-rpc-store` crate.

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done.

> Line numbers below are from this session's exploration and may drift;
> re-grep before editing.

## Handoff notes (read first)

Branch `embed-rpc-store`. Completed pieces and their commits (newest last):

- `141c212c33` 1A perpetual-store ingestion client (`sui-core`).
- `071d797539` 1B-i broadcast checkpoint stream.
- `b273008626` 1B-ii broadcast streaming client.
- `91c1920731` 1C-i `Db::clear_all`.
- `106b35e882` 1C-iii Synchronizer dynamic-membership snapshot coordinator.
- `0d96e830e2` 2A embedded cohort `PipelineLayer::embedded()` constructor.
- `aa5801e39b` 2B-i `epochs::start` / `seed_current_epoch_start` optional
  `start_checkpoint` (merge audit; proto/merge already presence-tracked).
- `6bf04f0987` 2B-ii `seed_history_cohort` + `LIVE_COHORT`/`HISTORY_COHORT`.
- `b3509e2542` 2B stamp the history-cohort pruning floor at `L`.
- `009c5e2ab5` 3A `use_experimental_rpc_store` flag on `RpcConfig`.
- `3141224969` 3C `RpcStoreReadStore` read path (`sui-core/src/storage.rs`).
- `4b1be8bfef` 3D-i `prune_history_cohort` (rpc-store side).
- `2fbfc1bdbb` 3D-ii thread `rpc_store` through `AuthorityStorePruner`.
- `c351846a01` 3B-i `rpc_store` param on `AuthorityState::new` -> pruner.
- `6dfed12216` 3B-ii `EmbeddedRpcStore` orchestrator (`rpc_store_embed.rs`) +
  `decide` unit tests.
- `d0e1117b37` 3B-iii sui-node startup wiring (bootstrap, read path, spawn).
- `5acd4b6a27` 4-fix startup deadlock (background-spawn the indexer) + broadcast
  streaming client tip-seed.
- `f2683e87a5` 4-fix subscription service gates checkpoint delivery on indexing.
- `7672250124` 4-fix balance reader reports the coin half, not the total.
- `2fc998ec56` 4A expose embedded store bootstrap action + cohort watermarks
  (`SuiNode::embedded_rpc_store`, `Bootstrap` pub).
- `fab70e9e25` 4A restore test harness + resume case (`tests/rpc/restore.rs`).
- `c2515768e1` 4A enable-indexing-rebuild case (`Restore { clear: false }`).
- `d353078564` 4A resume-and-catch-up-after-indexing-gap case.
- `83a8e41f3c` 4A pruned-range-forces-rebuild case (`Restore { clear: true }`).
- `4317f0af2d` 4-fix atomic `highest_committed_checkpoint` watermark (restore
  target vs live-object-set consistency; balance double-count on unclean restart).

`git log --oneline main..HEAD` shows the series (plus `*: tick ... embed plan`
doc commits). Each commit is self-contained with tests + clippy + fmt green.

The dynamic-membership coordinator was exercised live: a standalone
`sui-rpc-node run` caught up at ~448 ckpt/s (range 247-639 over a 2-min
window), with the coordinator keeping pace with ingestion (committed frontier
~= ingested frontier; no barrier starvation). Scraper saved at
`consistent-store-testbed/scrape_rate.py`. Note: standalone keeps every
pipeline a cohort member, so late-join itself is only exercised once the
embedded cohort split lands (2B watermark seeding).

**Phase 1 is complete.** The two open questions were resolved: implement behind
unit tests (not a spike), and the concurrency primitive is a `std::sync::Mutex`
over membership/arrival counts plus a `tokio::sync::watch<u64>` carrying the
frontier (single source of truth + lost-wakeup-free wakeup). The fixed-size
`Barrier` is gone; lagging pipelines commit freely and late-join the cohort at
the frontier; a member whose channel closes on shutdown departs so peers do not
deadlock. See `SnapshotCoordinator` in `synchronizer.rs`.

**Phase 4A is done.** The temp flag flip is reverted (it was never committed;
the default is back to `unwrap_or(false)`). Bespoke restore tests opt into the
embedded backend explicitly through the existing
`TestClusterBuilder::with_rpc_config` ->
`SwarmBuilder::with_fullnode_rpc_config` -> `FullnodeConfigBuilder::with_rpc_config`
threading (a full `RpcConfig` with `use_experimental_rpc_store: Some(true)`), so
no new builder toggle was needed. Phase 4 *validation* against the embedded
backend (the 90/90 run) surfaced and fixed three runtime bugs (`5acd4b6a27` /
`f2683e87a5` / `7672250124`); a fourth -- a balance double-count across an
unclean restart -- was found by the new restore tests and fixed with the atomic
`highest_committed_checkpoint` watermark (`4317f0af2d`); see "Phase 4" below.

**Next up: Phase 5 (ledger-history availability exposure).**

How to run the suite: `cargo nextest run -p sui-e2e-tests --test rpc -j 2
--no-fail-fast` (cap concurrency at ~2; each test spins a `TestCluster`). For a
single test add `-E 'test(<name>)'`; for just the restore cases,
`-E 'test(restore::)'`. To debug a hang, add
`telemetry_subscribers::init_for_testing();` as the first line of the test and
run with `--no-capture` + `RUST_LOG="error,sui_core::rpc_store_embed=
debug,sui_rpc_store=debug,sui_indexer_alt_framework=debug"` under a `timeout`.

The 3B orchestrator lives in `sui-core::rpc_store_embed` (not `sui-node`, which
lacks the rpc-store / consistent-store / framework deps). It composes the
already-landed building blocks: `restore_indexes(.., RestoreLayer::indexes_only(),
..)` (live cohort -> `T`), `seed_history_cohort(.., Watermark for L-1, ..,
Some(&perpetual as &dyn ObjectStore))` (history cohort -> `L-1`, partial epoch
seed), and `Indexer::from_store(..)` + `add_pipelines(PipelineLayer::embedded(),
..)` + `run()` wired to the 1A ingestion + 1B broadcast clients. Note on the
history seed point: `seed_history_cohort` treats the seed watermark as "committed
through", so the backfill resumes at -- and the pruning floor sits at --
`seed.checkpoint_hi_inclusive + 1`. To make checkpoint `L` the lowest available,
`sui-node` passes the watermark for `L - 1` (with `tx_hi` = tx count through
`L - 1`, read from checkpoint `L - 1`'s summary). When `L == 0` the history
cohort is left unseeded so it backfills from genesis. Still deferred to Phase 5:
advertising the *upper* bound of ledger-history availability while the backfill
is mid-flight.

**Workflow the user asked for:** one commit per sub-phase (1A, 1B, ...); ensure
`cargo nextest run -p <crate> <filter>`, `cargo xclippy` (from the crate dir),
and fmt all pass; then tick this doc. Commits: `scope: title` (e.g.
`consistent-store: ...`), no Co-Authored-By lines.

**rustfmt caveat (important):** local `rustfmt 1.8.0-stable` disagrees with the
repo's pinned formatter on ~100-char boundary lines and flags *pre-existing,
unmodified* code (e.g. `consistent-store/src/db.rs:277` in `open()`,
`options.rs`). Do NOT whole-crate `cargo fmt -p <crate>` (it would churn
unrelated boundary lines into the diff). Instead verify only your own added
lines: `cargo fmt -p <crate> -- --check 2>&1 | rg 'Diff in .*<file>:[0-9]+'`
and confirm no diff falls inside the lines you added. See
[[feedback-rustfmt-version-mismatch]].

## Goal

Embed `sui-rpc-store` into the fullnode to replace `sui-core/src/rpc_index.rs`,
using only a subset of the crate's column families (the index and history
surfaces). Raw chain data is NOT written into `sui-rpc-store` in this
configuration -- the perpetual store already has it. The new backend is gated
behind an experimental config flag and runs as an alternative to the existing
implementation (selectable per node) so we can validate it against the existing
e2e RPC tests before a hard cutover that eventually deletes `rpc_index.rs`.

## Locked decisions

- **Scope:** include the ledger-history bitmap indexes (full parity), not just
  the live-object-set indexes.
- **Read consistency:** keep the Synchronizer and its cross-pipeline
  consistency window (do not bypass it).
- **Backfill model:** the history cohort is seeded at the lowest-available
  checkpoint `L` and backfills upward on a single per-pipeline watermark. The
  Synchronizer gains a dynamic-cohort "late-join": the live cohort holds the
  snapshot frontier at the tip; lagging history pipelines commit freely and
  join the snapshot barrier once they climb to the frontier. Known consequence:
  recent ledger-history queries are unavailable until the backfill reaches the
  tip (availability exposure handled in the deferred final phase).
- **Subscription service under broadcast lag:** on a `Lagged`/gap event, tear
  down all in-progress subscriptions (clients already tolerate connection
  breaks and reconnect) and resume from the next received checkpoint. Delete
  the current out-of-order panic.
- **Config flag:** `use_experimental_rpc_store` on `RpcConfig` (default
  `false`); mutually exclusive with building the old `rpc_index`.
- **`live_objects`:** keep populating it. Reads use the perpetual store for
  latest-version lookups, but `live_objects` is retained so we can serve it
  from consistent snapshots in the future if needed.
- **`tx_seq` <-> `digest`:** maintain both directions (`tx_seq_by_digest` and
  `tx_metadata_by_seq`) -- both are needed to interpret bitmap query results.
- **Ledger-history availability exposure:** DEFERRED to the final phase, after
  everything else is working.

## Cohort model (embedded config)

**Live cohort** -- restored to tip `T`, follows live from `T + 1`:
- `live_objects`, `object_by_owner`, `object_by_type`, `balance`,
  `package_versions`.

**History cohort** -- watermark seeded to `L` (lowest available), backfills
upward:
- `epochs` -- the pipeline backfills historical epoch rows from `L`. Restore
  also writes a PARTIAL current-epoch seed so tip type-layout and committee
  reads work immediately. See "Epoch seed (partial)" below -- unlike the
  formal-snapshot restore, the perpetual-store restore lands mid-epoch (at tip
  `T`, not an epoch boundary), so we can only seed the fields derivable from
  the mid-epoch `SuiSystemState`, not the full start record.
- `transaction_bitmap`, `event_bitmap`.
- `tx_metadata_by_seq`, `tx_seq_by_digest`.

**Deactivated** -- the perpetual store serves these, so they are not written:
- `transactions`, `effects`, `events`, `objects`, `checkpoint_summary`,
  `checkpoint_contents`, `checkpoint_seq_by_digest`.

One framework `Indexer` + one `Synchronizer` over the active cohorts. The
reader composes: index lookups (`object_by_owner` / `object_by_type` etc.)
yield `(ObjectID, version)`, and the object bytes are loaded from the perpetual
store; `live_objects` is kept in sync but not on the read hot path.

## Phases and tasks

### Phase 1 -- foundations (parallel, land first)

- [x] **1A. Perpetual-store ingestion client**
  (`sui-core/src/rpc_store_ingestion_client.rs`). Implemented
  `PerpetualStoreIngestionClient<R>`, generic over `ReadStore` (production `R`
  = `RocksDbStore`; generic bound keeps it unit-testable against an in-memory
  mock). Implements `IngestionClientTrait`:
  - `chain_id()` -> captured at construction (genesis may be pruned away).
  - `checkpoint(seq)` -> `get_checkpoint_by_sequence_number` +
    `get_checkpoint_contents_by_digest` + `ReadStore::get_checkpoint_data`
    (cleaner than `load_checkpoint`, which is `pub(crate)` and needs the
    cache reader); missing/pruned -> `NotFound`.
  - `latest_checkpoint_number()` -> `get_latest_checkpoint_sequence_number`
    (= highest executed on `RocksDbStore`).
  - Added `sui-indexer-alt-framework` (default-features=false) to `sui-core`
    deps (no cycle). `from_trait` wrapping happens at the wiring site (3B).
  - 5 unit tests (NotFound, summary-without-contents, round-trip, latest,
    chain_id) green; clippy + fmt clean.

- [x] **1B. Broadcast checkpoint stream + streaming client** (landed as two
  commits). Channel type chosen: `broadcast::Sender<Arc<Checkpoint>>` (Arc to
  keep fan-out cheap across multiple subscribers).
  - [x] **1B-i. Broadcast conversion** (`sui-core` / `sui-rpc-api` /
    `sui-node` / `sui-fork`). Executor field + `new_for_tests`(None) +
    enqueue now non-blocking sync `send(Arc::new(..))`; the commit/enqueue
    helper is no longer async. `SubscriptionService::build` creates a
    `broadcast::channel`; `start()` matches `recv()` -> Ok / Lagged / Closed.
    New `handle_lag` drops all in-progress subscriptions and resets the
    in-order tracker; the out-of-order panic stays (now only reachable on a
    true executor bug). `sui-fork`'s `Context` + `rpc_executor` test harness
    updated to broadcast. 3 subscription unit tests + 95 sui-fork tests green.
  - [x] **1B-ii. Broadcast streaming client**
    (`sui-core/src/rpc_store_streaming_client.rs`). `BroadcastStreamingClient`
    implements `CheckpointStreamingClient`: `connect()` subscribes and yields
    a `Peekable<BoxStream>` via `stream::unfold`; `Lagged` -> stream error
    (framework reconnects and fills the gap from 1A's ingestion client),
    `Closed` -> stream ends. 3 unit tests (in-order, lag-errors, close) green.
    Wiring of both clients into the indexer happens in 3B.

- **1C. `sui-consistent-store` primitives.**
  - [x] **1C-i. `Db::clear_all()`** (`db.rs`). Wipes every CF (user +
    framework + default) back to empty in place, preserving each CF's options
    (merge operators / compaction filters stay attached because CFs aren't
    dropped), clears in-memory snapshots. Implemented as a whole-keyspace
    range delete per CF + compaction. Note in the doc: a `drop_cf`-based wipe
    would be O(file count) instead of O(data size) but would need to recreate
    CFs with schema options (generic over `Schema` + `RocksDbConfig`);
    deferred as a possible optimization (kept the simpler impl per review). 2
    unit tests green; clippy clean; canonical `cargo fmt` flags only
    pre-existing `open()` code (local rustfmt 1.8.0 boundary nit), not the new
    code.
  - [x] **1C-ii. Per-pipeline watermark seeding -- NO new code needed.** The
    framework CFs are public `DbMap`s (`db.framework().watermarks`), and
    `PipelineTaskKey::new` + `Watermark::for_checkpoint` are public, so 2B can
    seed live cohort -> `T` and history cohort -> `L` directly via a `Batch`.
    Plan: register only the live cohort with the restore driver (gets `T` via
    `complete_restore`); seed history watermarks to `L` with a direct
    `batch.put(&fw.watermarks, key, Watermark::for_checkpoint(L))`.
  - [x] **1C-iii. Synchronizer dynamic-cohort late-join** (`106b35e882`).
    Replaced both fixed-size `tokio::sync::Barrier`s with a
    `SnapshotCoordinator`. `run()` classifies each pipeline at startup: those
    within one stride of the frontier (`watermark >= current_window_start`,
    where `current_window_start = (init_checkpoint / stride) * stride`) are
    initial cohort members; pipelines lagging further behind (the history
    cohort seeded to `L`) start outside the cohort. The per-pipeline task
    reads the shared frontier each loop: below it -> commit freely; at it ->
    `arrive()` (joins the cohort if not already a member, then blocks until
    the snapshot is taken); past it -> bail (unreachable for a well-behaved
    pipeline). The last member to arrive takes the snapshot and advances the
    frontier. A late join is consistent because the joiner has committed
    through `frontier - 1` when it arrives. Concurrency: `std::sync::Mutex`
    over `{ members, arrived }` plus a `tokio::sync::watch<u64>` carrying the
    frontier (source of truth + wakeup; `watch` retention avoids lost
    wakeups). Added `depart()` so a member whose channel closes on shutdown
    leaves the cohort and releases any peers parked at the frontier (the
    original masked this shutdown deadlock via `JoinSet` abort). 2 focused
    tests added (caught-up cohort snapshots while a laggard is never fed;
    laggard joins on catch-up so a later snapshot waits for both); all 251
    crate tests + clippy + fmt green.

### Phase 2 -- indexer embedding glue (`sui-rpc-store`)

- [x] **2A. Embedded cohort wiring** (`0d96e830e2`). Added
  `PipelineLayer::embedded()` (`config.rs`) enabling exactly the ten cohort
  pipelines -- live: `live_objects`, `object_by_owner`, `object_by_type`,
  `balance`, `package_versions`; history: `epochs`, `tx_seq_by_digest`,
  `tx_metadata_by_seq`, `transaction_bitmap`, `event_bitmap` -- and leaving the
  seven raw-chain-data CFs `None`. `indexes_only()` (six pipelines) was too
  narrow (no `live_objects`, `epochs`, or `tx_seq`/`tx_metadata` maps), so a new
  constructor was the right call rather than widening it. No change needed to
  `Indexer::from_store` / `add_pipelines`: they already register only `Some(_)`
  pipelines with both the framework indexer and the synchronizer
  (`register_pipeline`), and `from_store` already accepts the 1A `IngestionClient`
  (via `from_trait`) + optional 1B `BoxedStreamingClient`. The live/history
  split is made by the coordinator from each pipeline's watermark at startup, not
  by the layer. 2 tests added (constructor set; `add_pipelines` registers exactly
  the ten); all 176 crate tests + clippy + fmt green.

- [x] **2B. Embedded restore + watermark seeding** (`aa5801e39b`, `6bf04f0987`).
  All in `sui-rpc-store`, no `sui-core` dep. `restore_indexes` was already
  generic over `RestoreSource`, so `sui-node` injects
  `PerpetualStoreRestoreSource` directly -- no change needed there. Added
  `seed_history_cohort(db, schema, history_watermark, chain_id, objects)`
  (`restore.rs`), the embedded analog of `floor_unrestored_pipelines`: writes
  `__watermark = L` + `__chain_id` for each history-cohort pipeline and, given an
  `&dyn ObjectStore`, the partial epoch seed. The caller (`sui-node`) passes its
  perpetual store as the `ObjectStore`, keeping this crate `sui-core`-free. Stamps
  the pruning floor at the lowest available checkpoint `L`
  (`checkpoint_lo = seed.checkpoint_hi_inclusive + 1`,
  `tx_seq_lo = seed.tx_hi`), mirroring `floor_unrestored_pipelines`; the backfill
  only writes `tx_seq` at or above the floor, so the bitmap compaction filter
  drops nothing it produces (the *upper* availability bound during backfill is
  still Phase 5). Added `LIVE_COHORT` /
  `HISTORY_COHORT` consts as the single source of truth for the split, exported
  from `lib.rs`, with the embedded-layer test retargeted to pin
  `PipelineLayer::embedded()` to their union via the real registered
  `Processor::NAME`s. Tests: epochs merge audit (partial seed + backfill don't
  clobber), `seed_history_cohort` unit test, Simulacrum partial-epoch integration
  test. All 180 crate tests + the rpc-node seed tests + clippy + fmt green.

  - **Epoch seed (partial).** Done via `seed_current_epoch_start(.., None, ..)`.
    The mid-epoch `SuiSystemState` (object `0x5`) supplies epoch, protocol
    version, gas price, start timestamp, and BCS; `start_checkpoint` is left
    unset because the epoch's first checkpoint precedes `T`. The upward backfill
    fills it iff that boundary falls in `[L, T]`.
  - **Merge implication.** AUDITED -- no change needed. The `epochs` proto
    already declares every field `optional` and the merge operator copies only
    fields present on an operand, so an unset `start_checkpoint` is presence-
    tracked, not a `0` sentinel. The only change was making the `epochs::start`
    builder accept `Option<u64>` so the partial seed can pass `None`; two merge
    tests pin the non-clobbering behavior in both orders.

### Phase 3 -- fullnode integration (`sui-node` / `sui-core`)

- [x] **3A. Config flag** (`009c5e2ab5`). Added `use_experimental_rpc_store:
  Option<bool>` to `RpcConfig` (`rpc_config.rs`; next to `enable_indexing`)
  with accessor, default `false`. Selecting it builds the new backend and
  skips the old `rpc_index`.

- [x] **3B. Startup orchestration** (`c351846a01`, `6dfed12216`, `d0e1117b37`).
  The orchestrator lives in `sui-core` (`rpc_store_embed::EmbeddedRpcStore`), not
  `sui-node`, because `sui-node` lacks the `sui-rpc-store` /
  `sui-consistent-store` / framework deps that `sui-core` already has; `sui-node`
  only handles `sui-core` types (no new deps). `EmbeddedRpcStore::bootstrap`:
  1. Opens the rpc-store `Db` under `db_path()/rpc_store`.
  2. Computes `L = max(object_pruned, checkpoint_pruned) + 1` (0 if nothing
     pruned) and `T = highest_executed` (`None` on a fresh node's first boot --
     genesis is executed later in startup).
  3. Reads the per-pipeline watermarks (per cohort) and the `__chain_id`.
  4. The pure `decide` helper branches: **resume** when both cohorts resume `>=
     L`; **seed history only** when live is in range but history is below `L`;
     **restore** (clear first on wrong-chain or live-out-of-range) otherwise.
     Live cohort restores to `T` (`restore_indexes` + blocking `Service::join`);
     history seeds to `L - 1` (`seed_history_cohort`), or is left unseeded when
     `L == 0` so it backfills from genesis (an unwatermarked pipeline resumes at
     `first_checkpoint = 0`); a fresh node (`T == None`) skips the bulk-load and
     builds both cohorts from genesis. `decide` has unit-test coverage of the
     in-range / uninitialized / out-of-range / wrong-chain / genesis /
     history-behind-floor matrix (the 4B startup-decision-matrix test).
  5. `spawn_indexer` builds `Indexer::from_store(...)` wired to the 1A ingestion
     client + 1B broadcast streaming client (when the executor's broadcast
     sender exists) and retains the resulting `Service` on the handle.
  6. `sui-node` wiring (`d0e1117b37`): `start_async` bootstraps the embedded
     store in place of `rpc_index` (mutually exclusive), passes its store handle
     to `AuthorityState::new` (-> pruner, flipping the 3D `None` to `Some`), its
     reader to `build_http_servers` (-> `RpcStoreReadStore` read path), and spawns
     the tip indexer after `build_http_servers` returns (so the broadcast sender
     exists), holding the handle on `SuiNode` for the node's lifetime.

  Compiles + clippy + fmt clean; `decide` unit tests green. NOT yet validated at
  runtime end to end -- that is Phase 4 (test wiring + e2e suites against the new
  backend).

- [x] **3C. Read-path wrapper** (`3141224969`). Added `RpcStoreReadStore`
  (`sui-core`, sibling to `RestReadStore`) that serves objects / raw data /
  child resolution from the perpetual store and the `RpcIndexes` surface from
  the rpc-store reader (`reader/indexes.rs` implements `RpcIndexes`).
  Implements `ObjectStore`, `ReadStore`, `ChildObjectResolver`,
  `RpcStateReader`, `RpcIndexes`. Object/state available-range =
  `max(perpetual_low, rpc_store_low)`. Swapped in at the rpc-api wiring site
  when the flag is on. (Ledger-history-specific availability is Phase 5.)

- [x] **3D. Pruner integration** (`4b1be8bfef`, `2fbfc1bdbb`). rpc-store side
  (`4b1be8bfef`): `sui_rpc_store::prune_history_cohort(db, schema,
  pruned_checkpoint_watermark, pruned_tx_seq_exclusive)` prunes the HISTORY
  cohort only (live cohort and `epochs` are never pruned), idempotent on a
  re-run with the same or a lower floor. sui-core side (`2fbfc1bdbb`): threaded
  an `rpc_store: Option<RpcStore>` through the whole `AuthorityStorePruner`
  call chain and call it next to `rpc_index.prune` in both the `Objects` and
  `Checkpoints` passes on the shared retention floor. All call sites pass
  `None` until 3B builds the embedded store. We cannot use the rpc-store's
  standalone pruner Service -- there is no raw chain data here to drive it.

### Phase 4 -- tests

**Validation done: the `sui-e2e-tests` rpc suite (`--test rpc`, 90 tests) passes
90/90 against the embedded backend** (via the temp flag flip; see Handoff). The
run drove the suite from 32 -> 56 -> 88 -> 90 as three runtime bugs were fixed:

- **`5acd4b6a27` startup deadlock + streaming tip-seed.** (a) `spawn_indexer`
  built the indexer inline in `start_async`; `Indexer::from_store` blocks on
  `latest_checkpoint_number`, which can't resolve until the checkpoint executor
  runs -- and the executor only starts after `start_async` returns. Fixed by
  building/running the indexer on a background task whose handle
  (`EmbeddedRpcStore::indexer_task`) is aborted on drop. (b) The framework
  broadcaster `peek()`s the streaming stream to learn the tip before ingesting,
  but a fresh `tokio::broadcast` subscription only carries *future* checkpoints,
  so on an idle chain it blocked forever and ingested nothing. Fixed:
  `BroadcastStreamingClient` now takes a `ReadStore`, seeds the stream with the
  current tip read from the local store, and overrides `latest_checkpoint_number`
  to read the local store. `MockReadStore` moved to a shared
  `sui-core::rpc_store_test_utils` `#[cfg(test)]` module.
- **`f2683e87a5` read-after-write consistency.** The legacy index was committed
  synchronously by the executor before the checkpoint was enqueued to the
  subscription service, so `execute_transaction_and_wait_for_checkpoint`
  guaranteed a current index. The embedded indexer commits async, so delivery
  raced indexing. Fixed: `SubscriptionService::build` takes an optional
  `IndexedCheckpointFn`; `handle_checkpoint` holds a checkpoint back until the
  index has committed it (bounded by a timeout). The gate lives in the
  subscription service, NOT the executor enqueue (the indexer consumes the same
  broadcast, so gating the enqueue would deadlock it). The signal is the LIVE
  cohort's committed watermark (`EmbeddedRpcStore::indexed_checkpoint_fn`), not
  min-across-all -- the history cohort backfills independently and would block
  delivery on a restored node.
- **`7672250124` balance double-count.** The reader's `get_balance` /
  `balance_iter` set `BalanceInfo.coin_balance = Balance::total()` (coin +
  address) instead of just the coin half; the caller sums the two halves, so the
  address (accumulator) balance was counted twice. Fixed to report `b.coin`.
  (`derive_detailed_balance_changes_2` is correct -- legacy uses it identically.)

- [x] **4A. Test wiring + revert the temp flag flip.** The temp flip was
  reverted (uncommitted, so just discarded; default back to `unwrap_or(false)`).
  No new builder toggle was needed: the existing
  `TestClusterBuilder::with_rpc_config` (~1426) ->
  `SwarmBuilder::with_fullnode_rpc_config` (~225) ->
  `FullnodeConfigBuilder::with_rpc_config` (~363) threading already lets a test
  select the backend with a full `RpcConfig`. Bespoke restore tests opt in via
  `RpcConfig { enable_indexing: Some(true), use_experimental_rpc_store: Some(true),
  ledger_history_indexing: Some(true), .. }`. CI: the existing rpc suite runs
  against the legacy backend (the default); the new `restore` module is the
  embedded-backend coverage. Whether to also run the broader suites against the
  embedded backend in CI (a separate job) is still open. Other index-dependent
  suites to consider: `authenticated_events_*`, `address_balance_rpc_tests.rs`.

  In-memory monitoring (no RPC surface): `SuiNode::embedded_rpc_store()` exposes
  the `EmbeddedRpcStore`, whose `bootstrap_action()` reports the startup decision
  (`Bootstrap::{Resume, SeedHistory, Restore { clear }}`, now `pub`) and
  `live_committed_checkpoint()` / `history_committed_checkpoint()` report each
  cohort's `min(checkpoint_hi_inclusive)`.

  Restore tests live in `sui-e2e-tests/tests/rpc/restore.rs` (a dedicated
  fullnode restarted with a mutated `NodeConfig.rpc` over a stable `db_path`;
  reads use transient `SuiNodeHandle`s so a stop releases RocksDB locks). Four
  cases: resume-no-restore, enable-indexing-rebuild (`Restore { clear: false }`),
  resume-and-catch-up-after-gap, and pruned-range-forces-rebuild
  (`Restore { clear: true }`). Each asserts `GetBalance` (live `balance` index)
  and `ListTransactions` (history `transaction_bitmap` index) answer correctly
  afterward. They pass under both `cargo nextest` and `cargo simtest` (validated
  across seeds 1-20 plus repeats).

  **Simtest restart race (`21a1214239`, `69a88d1c04`) -- full investigation.**
  Restarting a node on the same `db_path` reopens the previous instance's RocksDB
  databases, which fails if their file locks are still held. Under a real runtime
  `Node::stop` joins the node's thread, so everything is dropped before it
  returns. Under `cargo simtest` the stop only schedules teardown
  (`delete_node`), and the teardown -- the node's background tasks plus RocksDB's
  destructor, which runs on `spawn_blocking` *real* threads -- finishes a
  non-deterministic time later. The simulator is otherwise deterministic per
  seed, but this real-thread teardown leaks real-time scheduling in, so **the
  same seed passes or fails across runs** (observed: seed 10 went pass/pass/fail
  over three runs). CI hit it under assorted seeds.

  The conflict surfaces two different ways depending on which DB loses the race,
  because the stores open differently:

  - **committee / perpetual / checkpoint** use `#[derive(DBMapUtils)]`, whose
    generated `open_tables_read_write` calls `.expect("Cannot open DB at ...")` ->
    a **panic** at `committee_store.rs:24` (un-retryable). The committee store's
    `live/epochs` DB is the usual loser here.
  - the **embedded rpc-store** opens via `sui_consistent_store::Db::open(...)`
    `.context("opening the embedded rpc-store database")?` -> returns an **`Err`**,
    which the swarm container unwraps -> panic at `container-sim.rs:63`. This one
    lingers past the `SuiNode` because the tip indexer task holds its own `Db`
    clone (and does its writes via `spawn_blocking`), released only as the aborted
    task winds down.

  Approaches tried, and why a wait cannot be made reliable:

  - **Fixed sleep** (100ms, then 5s): racy. The sleep is *simulated* time; the
    RocksDB close runs in *real* time, so any fixed value flakes (100ms failed
    seed 2; 5s failed seed 8).
  - **`Weak<SuiNode>` wait**: necessary but insufficient. It covers the
    `SuiNode`-owned DBs (committee/perpetual/checkpoint -- the panicking ones the
    retry *cannot* catch), but the indexer's rpc-store `Db` outlives the
    `SuiNode`, so the rpc-store still raced.
  - **`DbRef` wait** (`Db::downgrade()` -> `Weak<DbInner>`): a more precise signal
    -- it tracks the rpc-store `Db` itself, including the indexer's clone -- but
    still 5/30 failures. `upgrade()` observes the `Arc<DbInner>` *strong count*,
    which hits 0 the instant the last clone drops; but that last drop happens on
    the `spawn_blocking` real thread, so the count reaches 0 *before* RocksDB's
    destructor releases the OS lock. The handle drop is not synchronous with the
    file-lock release.
  - **LOCK-file polling**: a dead end. The RocksDB `LOCK` file is **empty (0
    bytes), holds no data, and is never deleted** -- it is just the target of an
    `fcntl` advisory lock, which is kernel state with no on-disk reflection. Worse,
    under simulation the whole network is **one OS process**, and `fcntl` locks are
    per-process, so the lock does not even conflict at the OS level -- RocksDB
    rejects the reopen from its own in-process `locked_files` registry (hence the
    error text `lock hold by current process`). An external `fcntl`/`flock`/file
    probe from the same process cannot see that registry.

  Because the conflict ultimately lives in RocksDB's in-process registry (released
  only when the destructor finishes on the real thread), the one operation that
  can observe "this path is openable again" is asking RocksDB to open it. So the
  robust fix is a **bounded retry of `SuiNode::start`** in `container-sim.rs` (it
  retries until the registry entry is gone), paired with the **`Weak<SuiNode>`
  wait** for the panicking, un-retryable `DBMapUtils` stores. Validated 30/30
  (seeds 1-20 plus 10x the worst seed). A `DbRef`/LOCK-file wait is not a viable
  replacement for the retry; revisit only if the swarm gains a way to await a sim
  node's *full* deletion.

  **Bug found + fixed (`4317f0af2d`): restore-target vs object-set consistency.**
  The enable/rebuild test was ~50% flaky with a recipient balance reported at 2x.
  Root cause: the executor commits a checkpoint's objects (`commit_transaction_outputs`)
  and bumps the checkpoint store's `highest_executed` watermark in a *separate*
  write (with a `fail_point!("crash")` between). An unclean stop leaves the live
  object set ahead of `highest_executed`; the restore (which reads the live set)
  then stamped its watermark at `highest_executed`, so it captured the in-flight
  checkpoint's coins *and* the forward indexer re-applied them via the additive
  `balance` merge. Fixed by adding a `highest_committed_checkpoint` CF to
  `AuthorityPerpetualTables`, written in the same atomic batch as the outputs
  (new `ExecutionCacheCommit::set_highest_committed_checkpoint_in_batch` hook),
  and using it as the embedded restore target (always >= `highest_executed`).

- [~] **4B. Focused tests.** Done so far: startup decision matrix
  (`rpc_store_embed::tests`, the `decide` fn); subscription gate
  (`subscription::tests::handle_checkpoint_waits_for_index_before_delivering`);
  streaming-client tip-seed + `latest_checkpoint_number`
  (`rpc_store_streaming_client::tests`); ingestion-client round-trip
  (`rpc_store_ingestion_client::tests`); synchronizer late-join
  (`synchronizer.rs` tests). TODO: a focused test that the embedded read path is
  read-after-write consistent end to end (the e2e suite covers it, but a smaller
  test would localize regressions); subscription-service lag teardown already
  has a unit test (`handle_lag_drops_all_subscribers_and_resets_tracker`).

### Phase 5 -- deferred: ledger-history availability exposure

- [ ] **5A.** Decide and implement how the rpc-api advertises ledger-history
  availability (bounded by the history watermark during backfill) vs. the
  object/state availability range -- extend the availability surface or pick a
  single conservative value. Deferred until everything else works.

## Remaining work (consolidated tracker)

Everything still open, roughly in critical-path order toward the hard cutover
(make the embedded store the default and delete `rpc_index.rs`). Phases 1-4A are
done.

### Near-term hardening (pre-cutover)

- [ ] **Broaden embedded-backend validation.** The `tests/rpc` suite is the only
  e2e coverage running against the embedded backend (via the `restore` module's
  explicit opt-in). Run the other index-dependent suites against it too:
  `authenticated_events_*`, `address_balance_rpc_tests.rs`. Each needs the
  `with_rpc_config` opt-in (or a temporary global flip while validating).
- [ ] **CI strategy.** Decide whether the broader e2e suites run against the
  embedded backend as a separate CI job, and wire it up.
- [ ] **Pre-existing legacy flake.** `v2::state_service::list_owned_objects::
  test_reverse_sorted_coins_by_balance` fails rarely under `-j2` parallel load
  (passes 4/4 in isolation). It exercises the legacy `list_owned_objects` path,
  not the embedded store, so it predates this work -- investigate the
  read-after-write race, quarantine, or leave. Not a regression from the embed
  changes.
- [ ] **4B focused read-after-write test.** A small unit/integration test that
  the embedded read path is read-after-write consistent end to end (the e2e
  suite covers it; a smaller test would localize regressions). The rest of 4B is
  done -- see 4B above.

- [ ] **Simtest restart robustness without the container retry (optional).** The
  restore tests rely on a bounded `SuiNode::start` retry in `container-sim.rs`
  (see "Simtest restart race -- full investigation" under 4A for why a wait
  cannot replace it). A cleaner alternative would be a swarm primitive that
  awaits a sim node's *full* deletion (all tasks gone, all RocksDB destructors
  finished) so a restart never races -- and/or making the `DBMapUtils`
  `open_tables_read_write` return `Result` so committee/perpetual/checkpoint
  conflicts are retryable like the rpc-store. Both are broader test-infra /
  typed-store changes; the current retry + `Weak<SuiNode>` wait is robust (30/30)
  in the meantime.

### Atomic-watermark fix follow-ups (`4317f0af2d`)

- [ ] **Focused unit test** for `highest_committed_checkpoint`: that the executor
  stamps it atomically with `commit_transaction_outputs`, and that the embedded
  bootstrap reads it as the restore target. Currently only covered indirectly by
  the e2e restore tests.
- [ ] **Retire the `highest_executed` fallback** in `EmbeddedRpcStore::bootstrap`
  once databases written before this watermark existed no longer need support.
- [ ] **Confirm db-checkpoint/snapshot inclusion** of the new perpetual CF
  (almost certainly handled by `#[derive(DBMapUtils)]`, but unverified).

### Cutover (the end goal)

- [ ] **`ListBalances` zero-balance parity.** The embedded `balance` CF drops
  rows where both halves are zero, so `ListBalances` omits coin types held only
  as zero-value coins; legacy reports them as `balance: 0`. Decide whether to
  match legacy before cutover (a real behavioral divergence). See SUMMARY.md
  "Deferred work".
- [ ] **Hard cutover.** Flip the default to the embedded backend and delete
  `sui-core/src/rpc_index.rs`. Gated on the validation breadth, Phase 5, and
  zero-balance parity above.

### Crate-level deferred work (`sui-rpc-store`)

Tracked in full in `SUMMARY.md` "Deferred work"; mirrored here so this plan stays
a complete checklist:

- [ ] **Pruning follow-ups:** (a) optional `max_chunk_transactions` bound in
  `PrunerConfig`; (b) no real-chain multi-epoch pruner e2e test; (c) bitmap
  eviction of freshly-merged buckets can lag one compaction cycle.
- [ ] **Configurable backpressure:** thread the remaining `SequentialConfig`
  knobs (`min_eager_rows`, `max_pending_rows`, channel depths,
  `subscriber_channel_size`) through `CommitterLayer`.
- [ ] **Epoch-start seed only at restore finalize:** no retroactive backfill /
  in-place repair for a db restored before that change or via a path skipping the
  `objects` CF.
- [ ] **Concurrent pipelines with sequential gating:** let raw-chain-data
  pipelines run concurrent rather than sequential; needs a `sui-consistent-store`
  synchronizer change.
- [ ] **Cross-shard snapshot consistency (perpetual-store source):** fine today
  because fullnode restore is blocking; an online restore would need one rocksdb
  snapshot shared across all shard iterators. Distinct from the
  `highest_committed_checkpoint` fix, which addresses restore-target vs
  object-set consistency.
- [ ] **End-to-end tests still missing:** a restore test against the
  formal-snapshot source's real file layout, and an automated full
  real-chain-stream assertion.

## Landing order

1A, 1B, 1C in parallel -> 2A, 2B -> 3A (trivial), 3C, 3D -> 3B (the
integration) -> Phase 4 validation (done, 90/90) -> 4A harness wiring + revert
temp flip -> Phase 5.

## Key code anchors (current state, this session)

### Embedded-path code (added/changed this work)

- `EmbeddedRpcStore`: `sui-core/src/rpc_store_embed.rs` -- `bootstrap`, pure
  `decide` (resume / seed-history / restore), `cohort_committed` /
  `cohort_resume`, `spawn_indexer` (background task) + `build_indexer`,
  `store()` / `reader()` / `indexed_checkpoint_fn()`, `Drop` aborts the task.
- sui-node wiring: `sui-node/src/lib.rs` -- bootstrap branch where `rpc_index`
  is built (search `creating embedded rpc-store`); `AuthorityState::new`
  gets `embedded.store()`; `build_http_servers(.. embedded_rpc_store)` builds the
  read path (`RpcStoreReadStore` vs `RestReadStore`) and passes
  `indexed_checkpoint` to `SubscriptionService::build`; `spawn_indexer` called
  after `build_http_servers`; `_embedded_rpc_store` field on `SuiNode`.
- `RpcStoreReadStore`: `sui-core/src/storage.rs` -- `new(state, rocks, reader)`.
- Subscription gate: `sui-rpc-api/src/subscription.rs` -- `IndexedCheckpointFn`,
  `build(.., indexed_checkpoint)`, async `handle_checkpoint`, `wait_until_indexed`.
  Other `build` callers pass `None`: `sui-fork/src/{startup.rs,tests/subscription_e2e.rs}`.
- Balance reader fix: `sui-rpc-store/src/reader/indexes.rs` `get_balance` /
  `balance_iter` (report `coin` half, not `total()`).
- Clients / test mock: `sui-core/src/rpc_store_{ingestion,streaming}_client.rs`;
  shared `sui-core/src/rpc_store_test_utils.rs` (`#[cfg(test)] MockReadStore`).
- TEMP flag flip (uncommitted): `sui-config/src/rpc_config.rs`
  `use_experimental_rpc_store()` -> `unwrap_or(true)`.

### Pre-existing anchors

- `RpcIndexStore`: `sui-core/src/rpc_index.rs` -- struct ~1628, `new` ~1647,
  `index_checkpoint` ~2026, `commit_update_for_checkpoint` ~2047. Pruned from
  `authority_store_pruner.rs` ~220.
- `RestReadStore`: `sui-core/src/storage.rs` ~405; instantiated
  `sui-node/src/lib.rs` ~2597.
- `RpcStateReader` trait: `sui-types/src/storage/read_store.rs` ~626;
  `RpcIndexes` trait ~691.
- Available-range: `sui-rpc-api/src/reader.rs` ~245;
  `RocksDbStore::get_lowest_available_checkpoint` `storage.rs` ~130;
  `get_lowest_available_checkpoint_objects` `storage.rs` ~578.
- Checkpoint stream: sender field `checkpoint_executor/mod.rs` ~195; enqueue
  ~1065; `SubscriptionService::build` `subscription.rs` ~50; out-of-order
  panic ~107; wiring `sui-node/src/lib.rs` ~2593, executor construction ~1744.
- `Checkpoint` type: `sui-types/src/full_checkpoint_content.rs` ~204.
- `IngestionClientTrait`:
  `sui-indexer-alt-framework/src/ingestion/ingestion_client.rs` ~55;
  `from_trait` ~303; returns `full_checkpoint_content::Checkpoint`.
- `Synchronizer`: `sui-consistent-store/src/synchronizer.rs` -- struct ~97,
  `init_checkpoint = max` ~197, task loop ~230-323, snapshot barrier ~265-295,
  in-order ensure ~302; `Store::install_sync` `store.rs` ~150.
- Framework CFs / watermark: `sui-consistent-store/src/framework.rs`
  FrameworkSchema ~201; restore `complete_restore` `db/mod.rs` ~309,
  `restore_at` ~229; `Watermark` struct `db/mod.rs` ~84; `drop_cf` `db.rs`
  ~664.
- `PerpetualStoreRestoreSource`: `sui-core/src/rpc_store_restore_source.rs`.
- Pruner: `authority_store_pruner.rs` -- `prune_objects_and_indexes` ~140,
  `setup_pruning` ~826; started `authority.rs` ~3580. Perpetual pruned
  watermark `authority_store_tables.rs` ~122 (`get_highest_pruned_checkpoint`
  ~602); highest executed `checkpoints/mod.rs` ~582; `CheckpointWatermark`
  enum ~1183.
- Config: `RpcConfig` `sui-config/src/rpc_config.rs` ~9 (`enable_indexing`
  ~20); `NodeConfig.rpc` `node.rs` ~78; `AuthorityStorePruningConfig`
  `node.rs` ~1174.
- Test wiring: `FullnodeConfigBuilder` default rpc `node_config_builder.rs`
  ~648; `with_rpc_config` ~363; `SwarmBuilder::with_fullnode_rpc_config`
  `swarm.rs` ~225; `TestClusterBuilder::with_rpc_config` `lib.rs` ~1426.
- sui-rpc-store: `Indexer::from_store` (`indexer/mod.rs`),
  `PipelineLayer::all()` / `indexes_only()` (`config.rs`), `restore_indexes`
  (`indexer/restore.rs`), `RpcStoreReader` (`reader/mod.rs`), `RpcIndexes`
  impl (`reader/indexes.rs`).

## Open sub-questions (resolved ones struck through)

- ~~Exact flag name~~ -> `use_experimental_rpc_store`.
- ~~Keep `live_objects`?~~ -> yes, keep populating; reads use perpetual store
  for latest-version.
- ~~Both `tx_seq` <-> `digest` directions?~~ -> yes.
- ~~`epochs` cohort?~~ -> history cohort (backfill from `L`), with a PARTIAL
  current-epoch seed at restore (mid-epoch, so no `start_checkpoint`; see
  2B "Epoch seed").
- ~~Ledger-history availability now?~~ -> deferred to Phase 5.
- TBD during implementation: whether the history pipelines need any extra
  handling so a tip snapshot taken mid-backfill (history < frontier) never
  surfaces a false "as-of-C" ledger answer (tie-in with Phase 5).
