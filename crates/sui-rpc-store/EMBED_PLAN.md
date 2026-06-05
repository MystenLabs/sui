# Embedding sui-rpc-store into the fullnode (working plan)

Living plan + progress tracker for the `embed-rpc-store` work. Survives
context clears and compactions. See `SUMMARY.md` for the full design of the
`sui-rpc-store` crate.

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done.

> Line numbers below are from this session's exploration and may drift;
> re-grep before editing.

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

- [ ] **1A. Perpetual-store ingestion client** (`sui-core`, sibling to
  `rpc_store_restore_source.rs`). Implement `IngestionClientTrait`:
  - `chain_id()` -> `state.get_chain_identifier()`.
  - `checkpoint(seq)` -> assemble `full_checkpoint_content::Checkpoint`,
    reusing the existing `load_checkpoint` / `load_checkpoint_data` helper
    (`data_ingestion_handler.rs`); return NotFound outside `[L, T]`.
  - `latest_checkpoint_number()` -> highest executed checkpoint seq.
  - Wrap via `IngestionClient::from_trait(arc, metrics)`. Unit-test a
    round-trip.

- [ ] **1B. Broadcast checkpoint stream + streaming client**
  (`sui-core` / `sui-node` / `sui-rpc-api`).
  - Convert the executor->subscription channel from
    `mpsc::Sender<Checkpoint>` to `broadcast::Sender<Checkpoint>`
    (`checkpoint_executor/mod.rs` field ~195; enqueue ~1065 becomes
    non-blocking `send`, ignoring "no receivers").
  - `SubscriptionService::build` (`subscription.rs` ~50) creates a broadcast
    channel; the service holds a `broadcast::Receiver`. On `Lagged`/gap, drop
    all in-progress subscriptions and continue; delete the out-of-order panic
    (`subscription.rs` ~107).
  - New `CheckpointStreamingClient` (`BoxedStreamingClient`) over a
    `broadcast::Receiver`, lag handling modeled on `reconfig_observer.rs`; the
    framework backfills any gap via 1A.
  - Update node wiring (`sui-node/src/lib.rs` ~2593, executor construction
    ~1744) for the broadcast sender type.

- [ ] **1C. `sui-consistent-store` primitives.**
  - `Db::clear_user_data()` -- drop + recreate non-framework user CFs and
    reset `__watermark` / `__restore` / `__chain_id` for the
    "initialized but out of range" wipe-then-restore path. (Uses `drop_cf`,
    `db.rs` ~664.)
  - Per-pipeline watermark seeding API -- set the live cohort to `T` and the
    history cohort to `L` post-restore. Today only `complete_restore`
    (`db/mod.rs` ~309) writes `__watermark` (`framework.rs` FrameworkSchema
    ~201).
  - **Synchronizer dynamic-cohort late-join** (riskiest): replace the
    fixed-size `tokio::sync::Barrier` (`synchronizer.rs` task loop ~230-323;
    snapshot barrier ~265-295) with a dynamic-membership coordinator so
    snapshots only wait on pipelines that have reached the frontier. Lagging
    pipelines commit freely and join on catch-up. The cross-pipeline equality
    check (~302) relaxes to "joined pipelines at the frontier"; each pipeline
    still processes its own checkpoints strictly in order. `init_checkpoint =
    max(watermarks)` (~197) already seeds the frontier at the tip. Focused
    unit test: two pipelines, one lagging, snapshots advance on the tip cohort
    and the laggard joins on catch-up.

### Phase 2 -- indexer embedding glue (`sui-rpc-store`)

- [ ] **2A. Embedded cohort wiring.** Verify/extend `PipelineLayer`
  (`config.rs`) to express the cohort split (live + history active; raw chain
  data deactivated); confirm `indexes_only()` covers bitmaps + `tx_seq` maps +
  `epochs`, or add an `embedded()` constructor. Ensure `Indexer::from_store`
  (`indexer/mod.rs`) registers only the active cohort so the snapshot barrier
  covers exactly those pipelines, and accepts the 1A ingestion client + 1B
  streaming client.

- [ ] **2B. Embedded restore + watermark seeding.** Restore entry over
  `PerpetualStoreRestoreSource` (`sui-core`, exists) to bulk-load the live
  cohort, write the partial current-epoch seed (see below), then seed
  watermarks (live -> `T`, history -> `L`) via 1C. Wipe-then-restore via
  `clear_user_data` for the out-of-range case.

  - **Epoch seed (partial).** The formal-snapshot `seed_current_epoch_start`
    lands at an epoch boundary, so it seeds the full start record
    (`start_checkpoint = anchor + 1`, etc.). The perpetual-store restore lands
    at the tip `T`, which is mid-epoch, so we can only seed the fields
    derivable from the mid-epoch `SuiSystemState` (object `0x5`): epoch number,
    protocol version, reference gas price, `epoch_start_timestamp_ms`, and the
    `SuiSystemState` BCS. We CANNOT seed `start_checkpoint` (the epoch's first
    checkpoint precedes `T` and is not in the system state). The upward
    backfill fills `start_checkpoint` later iff that epoch's boundary falls in
    `[L, T]`; if the epoch started below `L` it stays absent.
  - **Merge implication.** The `epochs` merge must treat an unset
    `start_checkpoint` (and any other field the seed omits) as "unknown" --
    presence-tracked (proto `optional`), not a `0` sentinel -- so the partial
    seed and a later full backfill start record combine without either
    clobbering the other. Audit the `epochs` proto + merge operator for this
    before relying on it.

### Phase 3 -- fullnode integration (`sui-node` / `sui-core`)

- [ ] **3A. Config flag** (`sui-config`). Add `use_experimental_rpc_store:
  Option<bool>` to `RpcConfig` (`rpc_config.rs` ~9; next to `enable_indexing`
  ~20) with accessor, default `false`. Selecting it builds the new backend and
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

- [ ] **3C. Read-path wrapper** (`sui-core`, sibling to `RestReadStore`,
  `storage.rs` ~405; instantiated `sui-node/src/lib.rs` ~2597). New type that
  serves objects / raw data / child resolution from the perpetual store and
  the `RpcIndexes` surface from the rpc-store reader (`reader/indexes.rs`
  already implements `RpcIndexes`). Implements `ObjectStore`, `ReadStore`,
  `ChildObjectResolver`, `RpcStateReader`, `RpcIndexes`. Object/state
  available-range = `max(perpetual_low, rpc_store_low)`
  (`reader.rs` ~245; `storage.rs` ~130 / ~578). Swap in at ~2597 when the flag
  is on. (Ledger-history-specific availability is handled in Phase 5.)

- [ ] **3D. Pruner integration** (`sui-core`). Expose an rpc-store
  `prune(floor)` that prunes the HISTORY cohort only (live cohort is never
  pruned); wire it into `AuthorityStorePruner` next to `rpc_index.prune`
  (`authority_store_pruner.rs` ~140; constructed `authority.rs` ~3580) on the
  shared retention floor. We cannot use the rpc-store's standalone pruner
  Service -- there is no raw chain data here to drive it.

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
