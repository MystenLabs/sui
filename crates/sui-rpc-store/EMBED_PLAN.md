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

`git log --oneline main..HEAD` shows the series. Each commit is self-contained
with tests + clippy + fmt green.

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

**Next up: Phase 3B (startup orchestration in `sui-node` / `sui-core`).** Phase
2 and Phase 3A (config flag), 3C (read-path wrapper), and 3D (pruner
integration) are done; 3B is the only remaining Phase 3 piece. The
`sui-rpc-store` building blocks the embedded path needs all exist and stay free
of any `sui-core` dependency:

- `restore_indexes(db, schema, source, config, RestoreLayer::indexes_only(),
  metrics)` -- generic over `RestoreSource`, so `sui-node` injects
  `PerpetualStoreRestoreSource`. Bulk-loads the live cohort to `T`.
- `seed_history_cohort(db, schema, Watermark::for_checkpoint(L), chain_id,
  Some(&perpetual_store as &dyn ObjectStore))` -- seeds the history cohort to
  `L` + the partial epoch start record.
- `PipelineLayer::embedded()` + `Indexer::from_store(...)` +
  `add_pipelines(PipelineLayer::embedded(), committer)` + `run()`.

Phase 3 order (per landing order): 3A (config flag, trivial) -> 3C (read-path
wrapper) -> 3D (pruner integration) -> 3B (the startup orchestration that wires
restore_indexes + seed_history_cohort + the 1A/1B clients together). Note on the
history seed point: `seed_history_cohort` treats the seed watermark as
"committed through", so the backfill resumes at -- and the pruning floor sits at
-- `seed.checkpoint_hi_inclusive + 1`. To make checkpoint `L` the lowest
available, `sui-node` passes the watermark for `L - 1` (with `tx_hi` = tx count
through `L - 1`, read from the perpetual store's checkpoint `L - 1`). Still
deferred to Phase 5: advertising the *upper* bound of ledger-history
availability while the backfill is mid-flight.

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

- [ ] **3B. Startup orchestration** (`sui-node/src/lib.rs`, near the rpc_index
  creation ~679-696). When the flag is on:
  1. Open the rpc-store `Db` under `db_path()`.
  2. Compute the perpetual range: `L = max(checkpoint_store_pruned,
     object_store_pruned) + 1`, `T = highest_executed`.
  3. Read rpc-store per-pipeline watermarks and the `__chain_id`.
  4. Branch: **in range** -> open, no blocking, catch up forward;
     **uninitialized / behind** -> blocking restore + seed; **initialized but
     out of range** -> `clear_user_data` then restore + seed.
  5. Build `Indexer::from_store(...)` wired to 1A + 1B; spawn its `Service`.
  6. Do not build the old `rpc_index` when on.

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

- [ ] **4A. Test wiring.** Add a `TestClusterBuilder` toggle and flip the
  `FullnodeConfigBuilder` default (`node_config_builder.rs` ~648) so existing
  e2e RPC suites run against the new backend. Path:
  `TestClusterBuilder::with_rpc_config` (~1426) ->
  `SwarmBuilder::with_fullnode_rpc_config` (~225) -> `FullnodeConfigBuilder`.
  Suites: `crates/sui-e2e-tests/tests/rpc/v2/...`, `authenticated_events_*`,
  `address_balance_rpc_tests.rs`.

- [ ] **4B. Focused tests.** Synchronizer late-join; startup decision matrix
  (in-range / behind / out-of-range -> correct action); ingestion-client
  round-trip; subscription-service lag tears down in-progress subscriptions.

### Phase 5 -- deferred: ledger-history availability exposure

- [ ] **5A.** Decide and implement how the rpc-api advertises ledger-history
  availability (bounded by the history watermark during backfill) vs. the
  object/state availability range -- extend the availability surface or pick a
  single conservative value. Deferred until everything else works.

## Landing order

1A, 1B, 1C in parallel -> 2A, 2B -> 3A (trivial), 3C, 3D -> 3B (the
integration) -> Phase 4 -> Phase 5.

## Key code anchors (current state, this session)

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
