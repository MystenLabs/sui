# sui-rpc-store

This crate is the storage backend for `sui-rpc-api`. It consolidates three
storage surfaces that today live in three different places:

- **Raw chain data** — objects (every version), transactions, effects,
  events, checkpoint summaries / contents, committees. Today served by
  the validator's perpetual / checkpoint / committee stores via the
  `ReadStore` trait inside `sui-rpc-api`.
- **Derived indexes** — owner, type, balance, package version, epoch info,
  ledger-history bitmaps. Today split between `sui-core::rpc_index`
  (validator-side) and `sui-indexer-alt-consistent-store` (the framework
  indexer's own version).
- **Pruning watermarks** — driving bitmap compaction filters and
  `available_range` queries.

The goal is to land a *single* on-disk schema that backs every read
path `sui-rpc-api` cares about, indexed independently of the validator's
authority store. Once this crate is in production, `sui-rpc-api` reads
go through it instead of the perpetual / checkpoint / rpc_index stack,
and `sui-indexer-alt-consistent-store` can eventually be retired.

## Foundation: `sui-consistent-store`

`sui-rpc-store` is built on top of `sui-consistent-store`, a sui-internal
typed wrapper around RocksDB that provides:

- `Schema` trait — declare CFs (`name + rocksdb::Options`, the latter
  obtained from the `CfOptionsResolver` handed to `cfs`) and construct
  typed `DbMap<K, V, R>` handles in `open(db)`.
- `RocksDbConfig` + `CfOptionsResolver` — serde-friendly, per-CF
  tunable RocksDB options (see "RocksDB options" below).
- `DbMap<K, V, R>` — typed column-family handle parameterised by a
  `Reader` (defaulted to `Db`). Re-binds at snapshots via `.at(&snap)`.
- `Encode` / `Decode` — bespoke per-type traits for pinning on-disk byte
  layouts. `Protobuf<T>` wrapper delegates to `prost::Message`.
- `Batch` — atomic multi-CF puts / merges / point deletes /
  range deletes (`delete_range(map, from, to_exclusive)`, honored by
  reads immediately since `ignore_range_deletions` is left at
  RocksDB's `false`). `Db::compact_range_cf` forces a manual
  compaction (used by the pruner to evict bitmap buckets).
- `Snapshot` / `SchemaAtSnapshot` — point-in-time consistent reads.
- `metrics::ColumnFamilyStatsCollector` — a Prometheus collector
  exposing per-CF RocksDB stats (live size, compaction backlog,
  L0 files, `is_write_stopped`, …) as `cf_name`-labeled gauges. The
  `sui-rpc-node` binary registers it on the `run`, `serve`, and
  `restore` paths.
- `Store<S>` + `Connection<'_, S>` — implementations of the
  `sui-indexer-alt-framework` `Store` / `SequentialStore` traits, so
  framework-driven pipelines can commit through us.
- Framework CFs (`__watermark`, `__restore`, `__chain_id`) auto-registered
  on every open.
- `Synchronizer` — coordinates writes across multiple pipelines into one
  `Db` and takes cross-pipeline snapshots at stride boundaries.
- `Restore` trait + `RestoreDriver` + `RestoreSource` trait — bulk-load
  shape for pipelines, plus a generic driver that consumes a pluggable
  source. The built-in `FormalSnapshot` source restores from S3 / GCS /
  Azure / HTTP / local snapshots; `sui-core::rpc_store_restore_source`
  is the perpetual-store source for fullnodes bootstrapping from local
  state.

The contract for downstream crates like `sui-rpc-store` is:

1. Define the `Schema` (one struct of `DbMap` fields).
2. Define key / value types implementing `Encode` / `Decode`.
3. Define protobuf messages (if you want field-evolution on values).
4. Implement `Processor` + `sequential::Handler` for each pipeline.

Watermark persistence, restore, snapshots, and cross-pipeline atomicity
are inherited from the framework crate.

## Schema layout

19 CFs grouped by concern:

### Bookkeeping
- `pruning_watermark` — singleton `PruningWatermarks { tx_seq_lo,
  checkpoint_lo }`. Drives the bitmap CFs' compaction filters via a
  process-wide `Arc<AtomicU64>` (`pruning_watermark::tx_seq_floor`)
  refreshed from disk on startup. We dropped `object_lo` because the
  `(ObjectID, version)`-keyed `objects` CF has no meaningful
  monotonic low watermark.

### Per-epoch
- `epochs` — `EpochId → StoredEpoch`. Populated by a merge operator
  combining a *start* partial record (protocol version, gas price,
  start_ts, start_ckpt, BCS-encoded `SuiSystemState`) and an *end*
  partial record (end_ts, end_ckpt). Independent pipelines write each
  side; field-wise merge keeps them disjoint.
- **No `committees` CF** — the validator committee for an epoch is
  derived at read time from the `SuiSystemState` already in the
  `epochs` row via `SuiSystemStateTrait::get_current_epoch_committee`,
  which means we don't pay for two on-disk copies of the validator
  set.

### Per-checkpoint
- `checkpoint_summary` — signed checkpoint header
  (`CheckpointSummary` + `AuthorityStrongQuorumSignInfo`).
- `checkpoint_contents` — list of executed tx digests.
- `checkpoint_seq_by_digest` — `CheckpointDigest → checkpoint_seq`.

### Per-transaction
Transactions are keyed by an assigned `tx_seq` (u64 BE), not by
`TransactionDigest`, because:

- 32 B → 8 B per key per CF saves substantial space across `transactions`,
  `effects`, `events`, `tx_metadata_by_seq`.
- Sort order naturally matches global ordering (write locality,
  cheap range scans, clean `RangeDelete` pruning).
- The bitmap CFs already live in `tx_seq` space; keying data CFs the
  same way removes the impedance mismatch.

The digest-to-seq bijection lives in two CFs:

- `tx_seq_by_digest` — `TransactionDigest → tx_seq`.
- `tx_metadata_by_seq.digest` (carried inside the metadata row) for the
  reverse direction.

Data CFs:

- `transactions` — `TransactionData` + `Vec<GenericSignature>` as two
  BCS payloads (so callers can read or rewrite either side without
  reassembling the envelope).
- `effects` — `TransactionEffects` + `Vec<ObjectKey>` of
  unchanged-loaded objects (folded into the same row because rpc-api
  callers fetching effects almost always want both).
- `events` — `TransactionEvents`. Transactions that emitted no events
  get an empty `TransactionEvents` row rather than no row, so "row
  missing" unambiguously means "not yet indexed".
- `tx_metadata_by_seq` — typed `Metadata { digest, checkpoint_seq,
  ckpt_position, event_count, timestamp_ms }`.

### Object lifecycle
- `objects` — `(ObjectID, version) → StoredObject`. Every version ever
  observed; historical versions accrue.
- `live_objects` — `ObjectID → latest live version` (`U64Varint` value;
  no proto wrapper). The composed `get_object(id)` does
  `live_objects.get → objects.get` in one call.

### Indexes
- `object_by_owner` — `(OwnerKind, type, inverted_balance?, ObjectID) →
  version`. `OwnerKind` carries the owning `SuiAddress` inline for
  `AddressOwner` / `ObjectOwner`; `Shared` / `Immutable` carry no
  address. `ConsensusAddressOwner` collapses into `AddressOwner` so
  address-based listings don't split by consensus path. Coin-like
  objects encode the ones-complement of their balance (`!balance`) so
  richer coins sort first within `(owner, type)`. Iteration helpers:
  `iter_objects_owned_by_address`,
  `iter_objects_owned_by_address_of_type` (takes a `TypeFilter`),
  `iter_objects_owned_by_object`.
- `object_by_type` — `(StructTag, ObjectID) → version`. List every
  live object of a given Move type regardless of owner.
- `balance` — `(owner, coin_type) → BalanceDelta { coin, address }`.
  The two `i128` accumulators live in one row so a single read returns
  both halves; the merge operator sums field-wise with saturation; the
  compaction filter drops rows where both components are zero.
- `package_versions` — `(original_package_id, version) →
  PackageVersionInfo { storage_id }`. Walks every published version
  of a package.

### Ledger-history bitmaps
- `transaction_bitmap` — `(dimension_key, bucket) → BitmapBlob` over
  `tx_seq` space. `TX_BUCKET_SIZE = 65_536`.
- `event_bitmap` — same shape but over packed event-seq space:
  `packed = tx_seq << EVENT_BITS | event_idx`, `EVENT_BITS = 16`,
  `EVENT_BUCKET_SIZE = 1 << 28`.

Both share a `BitmapBlob` proto value (raw RoaringBitmap serialization
inside a `bytes` field). The merge operator unions every operand into
the existing accumulator and calls `.optimize()` to run-encode dense
containers before re-serializing. Compaction filters drop fully-pruned
buckets — the floor comes from `pruning_watermark::tx_seq_floor`,
shifted into packed event-seq space for `event_bitmap` (saturating to
`u64::MAX` if the shift would overflow).

### Removed by design
- **`committees`** — derived from the `epochs` row's stored
  `SuiSystemState`.
- **`coin_index`** — `CoinMetadata<T>`, `TreasuryCap<T>`, and
  `RegulatedCoinMetadata<T>` are all typed objects, so `object_by_type`
  resolves them via prefix scan.
- **`dynamic_fields`** — dynamic fields are stored as `Field<Name,
  Value>` objects whose owner is
  `Owner::ObjectOwner(parent_id_as_address)`, so a prefix scan on
  `object_by_owner` with `(ObjectOwner, parent)` enumerates them.
- **`VersionDigest`** (value of `object_by_owner` / `object_by_type`) —
  the digest is on the `Object` itself, so callers that need it can
  call `.digest()` after loading.
- **`LiveObjectRef.digest`** — same reasoning.
- **`StoredObject.checkpoint_seq`** — derivable via
  `Object.previous_transaction → tx_seq_by_digest → tx_metadata_by_seq`.

## Key bytes & encoding choices

- **`U64Be`** (8 B big-endian) for u64 *keys* whose sort order should
  match numerical order.
- **`U64Varint`** (1–10 B prost varint) for u64 *values* where size
  matters more than order. Used as the value type for
  `tx_seq_by_digest`, `checkpoint_seq_by_digest`, `live_objects`,
  `object_by_owner`, `object_by_type`. Saves ~3 B/row at typical
  mainnet `tx_seq` magnitudes vs `U64Be`.
- **Protobuf values with `bytes bcs` payload + metadata** — gives us
  field evolution on the wire while the BCS payload stays the canonical
  Sui-types byte sequence. Caller-facing reads decode the proto, then
  BCS-decode the canonical type.
- **`StructTagKey` / `TypeTagKey`** — bespoke streaming-parseable
  newtypes. Encode delegates to `bcs::to_bytes` so the bytes match
  canonical BCS (and sort the same), and decode is a hand-rolled
  streaming parser (uleb128 reader, Identifier reader, recursive
  TypeTag/StructTag) that consumes exactly one tag's bytes and leaves
  trailing bytes intact. This is what lets `object_by_owner`'s key
  interleave the variable-length `StructTag` between fixed-width
  prefix fields and a fixed-width suffix.
- **`TypeFilter`** — `Package(SuiAddress)`, `Module { package, module }`,
  `Type(StructTag)`. `Type` with empty `type_params` is special-cased
  in `encode_into` to drop the BCS uleb128 params-length byte, so a
  `Type(bare_struct_tag)` matches every instantiation (`Coin<SUI>`,
  `Coin<USDC>`, etc.). A `Type` with pinned params encodes the full
  BCS and matches only that instantiation. Used by `object_by_type`
  directly and by `object_by_owner`'s
  `AddressOwnerTypePrefix`.

## RocksDB options

RocksDB tuning is configuration-driven, resolved per column family.
The pieces span three crates:

- **`sui-consistent-store::options`** — `RocksDbConfig` (a
  database-wide `DbWideConfig`, a `default_cf` profile applied to
  every CF, and per-CF overrides keyed by name), `CfTuning`,
  `WriteStallConfig`, the `Compression` enum, and the
  `CfOptionsResolver`. Every field is `Option`, so three layers
  compose: the RocksDB native default (field unset), a code default,
  and a config override (`merge_over`). `Db::open` validates the
  config (write-stall ordering, unknown CF names) and builds one
  shared block cache, then hands a `CfOptionsResolver` to
  `Schema::cfs`. Each CF's `options()` calls `resolver.options(NAME)`
  for the resolved performance knobs and layers only its **merge
  operator and compaction filter** on top — those stay in code
  because they are correctness-bearing, not tunable.
- **`sui-rpc-store::default_rocksdb_config()`** — the baseline this
  crate ships. Ports `typed_store`'s production defaults (LZ4 with
  bottommost Zstd, 1 GiB write-buffer and WAL budgets, pipelined
  writes, a shared 1 GiB block cache, 8-way compaction parallelism)
  and bakes in a "no write stalls" policy: the pending-compaction
  stall limits are `0` (disabled) and the L0 slowdown/stop triggers
  raised to 512 / 1024, so neither the bulk restore nor steady-state
  indexing throttles on compaction debt while the stop trigger still
  bounds a runaway backlog. Per-CF deviations: a whole-key bloom
  filter on the point-lookup CFs (`tx_seq_by_digest`,
  `checkpoint_seq_by_digest`, `live_objects`) and a larger memtable
  on the bitmap CFs.
- **`sui-rpc-node` `[db]` config** — `DbConfig { snapshot_capacity,
  rocksdb }`. `to_db_options()` layers the operator's TOML overrides
  over `default_rocksdb_config()`; both `start_service` and
  `start_restorer` open through it, so **restore and tip indexing use
  one options path** (there is no restore-specific tuning).
  `generate-config` emits the fully populated defaults.

## Pipelines

One per CF (mostly — a few CFs share a pipeline and one CF is fed by two
pipelines). Each implements
`sui_indexer_alt_framework::pipeline::Processor` and
`sui_indexer_alt_framework::pipeline::sequential::Handler`. The shared
pattern:

- `Row { ... }` — pre-built typed value the commit path stages.
- `Batch = Vec<Row>` or `HashMap<Key, T>` — folds multiple checkpoints
  into one batch where useful.
- `commit` — tight loop of `conn.batch.put` / `delete` / `merge`.

Shared helpers in `indexer::mod`:

- `tx_seq_at(checkpoint, i)` — derives a transaction's `tx_seq` from
  `network_total_transactions - transactions.len() + i`.
- `checkpoint_input_objects` / `checkpoint_output_objects` — ported
  from `sui-indexer-alt-consistent-store::handlers`. Give the
  first-input and last-output state of each object across the
  *whole* checkpoint, which is what diff-based indexes need to
  retract the prior state before re-inserting the posterior.

Notable patterns:

- **Diff-based indexes** (`live_objects`, `object_by_owner`,
  `object_by_type`): emit a `Delete` for every input and a `Put` for
  every output. RocksDB applies the batch in order, so for objects
  that were merely modified the `Put` wins over the earlier `Delete`.
  For deleted/wrapped objects only the `Delete` lands.
- **`balance`**: per-transaction
  `derive_detailed_balance_changes_2(&tx.effects, &checkpoint.object_set)`
  returns one `DetailedBalanceChange` per `(address, coin_type)` per
  tx with both `coin_amount` and `address_amount` already computed.
  The pipeline forwards them straight to the schema's
  `balance::delta(owner, coin_type, coin, address)` helper that
  builds a single merge operand with both fields populated. This
  picks up address-balance changes from `AccumulatorWrite` events that
  an object-walking approach would miss.
- **`epochs`**: driven by `Checkpoint::epoch_info()`, which returns
  `Some(EpochInfo)` for the new epoch on the genesis checkpoint and
  on every end-of-epoch checkpoint. The pipeline emits a `Start`
  operand for the new epoch and (if not genesis) an `End` operand
  for the prior epoch with `end_timestamp_ms` taken from the new
  epoch's start (because the prior epoch ended at the moment the
  new one began) and `end_checkpoint = new.start_checkpoint - 1`.
- **Bitmap pipelines** (`transaction_bitmap`, `event_bitmap`): use
  `sui_inverted_index::for_each_transaction_dimension` and
  `for_each_event_dimension` to visit every dimension candidate,
  encode each `(dim, value)` with `encode_dimension_key`, and
  accumulate bits into `HashMap<(Vec<u8>, u64), RoaringBitmap>`.
  The handler's `batch` callback folds operands from multiple
  checkpoints, so commit stages at most one merge operand per
  `(dim_key, bucket)` per commit. Dimensions covered are Sender,
  AffectedAddress, AffectedObject, MoveCall (with prefix levels),
  EmitModule (with prefixes), EventType (with prefixes including
  type_params BCS), EventStreamHead, and EventExtant — exactly
  matching what `sui-core::rpc_index` indexes.

## Reading

All read methods live as `impl<R: Reader> RpcStoreSchema<R>` blocks in
each CF's module, returning canonical Sui types (decoded from the
proto + BCS payload) and wrapping errors as
`sui_consistent_store::error::Error`. Important entry points:

- `get_epoch(epoch)` / `get_committee(epoch)` (derived).
- `get_checkpoint_summary(seq)` → `VerifiedCheckpoint`.
- `get_checkpoint_contents(seq)`.
- `get_checkpoint_seq_by_digest(digest)`.
- `get_transaction(tx_seq)` → `(TransactionData, Vec<GenericSignature>)`.
- `get_tx_seq_by_digest(digest)`.
- `get_tx_metadata_by_seq(tx_seq)` → `Metadata`.
- `get_effects(tx_seq)` → `(TransactionEffects, Vec<ObjectKey>)`.
- `get_events(tx_seq)` → `TransactionEvents`.
- `get_object_by_key(id, version)`; `get_object(id)` (composed).
- `get_live_object_version(id)`.
- `iter_objects_owned_by_address(owner)`,
  `iter_objects_owned_by_address_of_type(owner, &TypeFilter)`,
  `iter_objects_owned_by_object(parent)`.
- `iter_objects_of_type(&TypeFilter)`.
- `get_balance(owner, coin_type)` → `Balance { coin, address }` with
  `.total()`; `iter_balances_owned_by(owner)`.
- `get_package_storage_id(original_id, version)`;
  `iter_package_versions(original_id)`.
- `get_transaction_bitmap(dim_key, bucket)` →
  `Option<RoaringBitmap>`;
  `iter_transaction_bitmap_buckets(dim_key)`.
- `get_event_bitmap(dim_key, bucket)`;
  `iter_event_bitmap_buckets(dim_key)`.
- `get_pruning_watermarks()`; `set_pruning_floor(tx_seq_lo)`;
  `refresh_pruning_atomics()`.

## Relationship to existing crates

- **`sui-consistent-store`** — storage primitives (`Db`, `Schema`,
  `DbMap`, `Batch`, `Snapshot`, `Store`/`Connection`). We declare the
  CF layout; this crate provides everything else.
- **`sui-indexer-alt-framework`** — drives the pipelines (`Processor`,
  `sequential::Handler`, ingestion service, watermark management).
- **`sui-inverted-index`** — dimension-extraction helpers
  (`for_each_transaction_dimension`, `for_each_event_dimension`,
  `encode_dimension_key`) for the bitmap CFs.
- **`sui-core::rpc_index`** — the validator-side implementation we're
  modelled after. Balance, bitmap, and epoch logic mirror it.
- **`sui-core::rpc_store_restore_source`** — `RestoreSource` impl over
  `AuthorityPerpetualTables`. Holds one rocksdb iterator per shard in
  a `spawn_blocking` task so the shard sees a single point-in-time
  snapshot of the perpetual store for the whole run.
- **`sui-indexer-alt-consistent-store`** — the framework-side
  indexer that this crate is eventually meant to replace. We reuse
  its per-CF module organisation and its
  `checkpoint_input_objects` / `checkpoint_output_objects` helpers.
  Its `restore/{formal_snapshot,format,storage,metrics,broadcaster,
  worker}.rs` modules were the basis for the ports under
  `sui_consistent_store::restore`.
- **`sui-rpc-api`** — the eventual consumer.

## Restore

Bulk-load path for the five live-object-derivable CFs
(`live_objects`, `object_by_owner`, `object_by_type`, `balance`,
`package_versions`). Operates separately from tip indexing: open
the database, run a restore against a source, then construct
the regular `Indexer` over the same store to start
tip-following. The `add_pipelines` restore-state guard accepts
pipelines that have transitioned to `Complete`.

`sui-rpc-store::restore_indexes(db, schema, source, config)` is
the entry point. It registers the five `Restore` impls on a
`sui_consistent_store::restore::RestoreDriver` and returns a
`Service` that drives the restore to completion.

After the restore, `floor_unrestored_pipelines` also seeds the
`epochs` row for the epoch the snapshot landed in (via
`seed_current_epoch_start`). The `epochs` pipeline only derives a
start record from an end-of-epoch checkpoint's `epoch_info`, and tip
indexing resumes at `anchor + 1` (past that checkpoint), so the row
would otherwise never get its protocol version, gas price, start
timestamp / checkpoint, or `SuiSystemState`. The seed reconstructs it
from the restored object set's on-chain `SuiSystemState` (epoch,
protocol version, gas price, start timestamp, BCS), with
`start_checkpoint = anchor + 1`. This keeps `get_epoch` /
`get_committee` and Move type-layout resolution (which reads the
latest epoch's `SuiSystemState` for the protocol version) working on a
freshly restored node. Runs only when the `objects` CF was restored;
best-effort otherwise.

### Driver shape (in `sui-consistent-store`)

- `RestoreSource` trait: streaming, sharded, resumable.
  `target_checkpoint() / target_watermark()` anchor the run;
  `shards() -> u32` declares how many disjoint slices the source
  exposes; `stream(shard_id, cursor)` returns a
  `BoxStream<RestoreChunk>` for that shard, resuming at
  `cursor + 1` if supplied. Each `RestoreChunk` carries a `Vec<Object>`
  and an opaque source-defined cursor.
- `RestoreDriver`: spawns one tokio task per shard (up to
  `shard_concurrency`). Per chunk, assembles one atomic `Batch`
  containing every registered pipeline's data writes plus each
  pipeline's `__restore` row update (`InProgress.shards[shard_id] =
  in_progress(chunk.cursor)`). On stream end the driver writes
  `ShardProgress::done` for the shard; once every shard is done
  for every pipeline, the finalizer atomically writes
  `RestoreState::Complete { restored_at: target_checkpoint }`,
  the matching `__watermark` row (so tip indexing resumes at
  `target_checkpoint + 1`), and the `__chain_id` row (so tip
  indexing refuses checkpoints from a different chain than the
  one we restored from).
- A single async mutex serialises the per-chunk commit step.
  Cross-shard parallelism comes from the source-fetch and stage
  phases; RocksDB's WAL serialises commits anyway so the mutex
  costs little beyond bookkeeping.
- `RestoreMetrics` (source-agnostic) exposes `restore_shards_total`
  / `restore_shards_done`. `shards_done` is derived from the
  persisted `__restore` cursors — each shard task increments it once
  when it confirms its shard is `Done`, including shards already
  complete on resume — so it reflects cumulative progress across a
  crash/restart, unlike the formal-snapshot source's per-process
  `total_partitions_fetched` counter.

### Per-shard state in `__restore`

`InProgress.shards` is a `BTreeMap<u32, ShardProgress>` where
`ShardProgress` is `oneof { bytes in_progress, Done done }`.
Three observable states per shard:

- Absent — never started; `stream(shard_id, None)`.
- `in_progress(cursor)` — resume with `stream(shard_id, Some(cursor))`.
- `done` — skip; the source's stream for this shard is exhausted.

Pipelines must be registered together — adding a new pipeline
mid-restore would leave its shard cursors empty while others
have advanced. Restore is one-shot; if you want to add a new
pipeline to an existing DB you either re-restore everything or
backfill from tip via the framework's existing mechanism.

### Built-in source: formal snapshot

`sui_consistent_store::restore::formal_snapshot::FormalSnapshot`
restores from the validator's per-epoch snapshot files
(S3 / GCS / Azure / generic HTTP / local). Every `.obj`
partition across all buckets is flattened (sorted by
`(bucket, partition)`) into one driver shard each, so
`shards() == partition count` (commonly ~1.4k for mainnet). This
lets the driver fetch partitions concurrently (bounded by
`shard_concurrency`, default 8 via the `[restore]` config / the
`--shard-concurrency` flag) and makes the driver's
`restore_shards_done` gauge track partition-level progress —
otherwise a single-bucket snapshot would be one giant sequential
shard with binary progress. Each partition is fetched and
committed as one chunk; its cursor is just present-or-absent
(present means the chunk committed, so a resume skips it rather
than re-applying additive index writes such as `balance`). Format
readers (`format.rs`), backend abstraction (`storage.rs`), and
metrics (`metrics.rs`) are ports from
`sui-indexer-alt-consistent-store::restore`. The
`target_watermark()` override surfaces the full epoch /
checkpoint / tx-count / timestamp tuple resolved from the
end-of-epoch checkpoint fetched via the supplied
`--remote_store_url`.

### Built-in source: perpetual store

`sui_core::rpc_store_restore_source::PerpetualStoreRestoreSource`
restores from a validator's `AuthorityPerpetualTables`. The
`ObjectID` space is split into 32 shards by the top 5 bits of
the first byte (matching `par_index_live_object_set`). Each
shard's stream is driven by exactly one `spawn_blocking` task
that opens one `range_iter_live_object_set` and pushes
`RestoreChunk`s of `CHUNK_SIZE = 50_000` objects over a bounded
tokio mpsc. RocksDB's implicit iterator snapshot pins a single
point-in-time view of the perpetual store for the whole shard
— `balance`'s merge semantics are safe even under concurrent
validator execution within a shard. The cursor is the 32-byte
`ObjectID` of the last object yielded; resuming starts at
`next_id(cursor)`. Trade-off: the SSTs the open iterator
references stay pinned for the run, so compaction is blocked
during restore. Fine for a one-shot bootstrap.

Cross-shard skew is still possible if the validator commits
between shard tasks starting, but every object lives in exactly
one shard so the rpc-store pipelines are unaffected. Fullnode
restore is blocking today (no execution during restore), so the
race doesn't arise in practice.

## Pruning

`indexer/pruner.rs` hosts a standalone background pruner, started as
a secondary `Service` from `Indexer::run` when `ServiceConfig.pruner`
is `Some`. It is *not* a framework pipeline: it reads already-committed
state and deletes history below a retention floor, modelled on the
validator perpetual-store pruner rather than the framework's
per-pipeline `prune` hook (the deletions are data-driven and the floor
is one value shared across every historical CF).

`start_pruner(db, config, metrics)` ticks every `interval_ms`, running
`prune_once` in a `spawn_blocking` task. Each pass:

1. **Target floor (epoch-based retention).** `current_committed_epoch`
   is the min `epoch_hi_inclusive` across the framework `__watermark`
   rows. `retention_checkpoint_floor` returns the `start_checkpoint`
   of the oldest retained epoch (`current_epoch - retention_epochs +
   1`); `None` (nothing to prune) when the chain is younger than the
   window or the epoch row is missing.
2. **Snapshot clamp.** `clamp_to_snapshot` holds the floor at or below
   `db.snapshot_range().start()`. Point and range deletes are already
   invisible to live snapshots (RocksDB pins the data they
   reference), but the bitmap compaction filter physically removes
   buckets irrespective of snapshots, so the clamp keeps every live
   snapshot's advertised range valid even under a tiny retention.
3. **Chunked advance.** Each tick advances the floor toward the target
   by at most `max_checkpoints_per_tick` checkpoints — so a large
   backlog (e.g. pruning newly enabled on an old DB) drains across many
   ticks rather than one long blocking pass; the floor converges over
   subsequent ticks. Within a tick the floor walks in atomic chunks of
   `max_chunk_checkpoints`. `prune_chunk` stages one atomic `Batch`:
   - **point deletes** — `objects` (effects-driven: each pruned tx's
     `modified_at_versions` + `all_tombstones`; the live version is
     never an input to a pruned tx, so it and the `live_objects`
     pointer survive), `tx_seq_by_digest` (tx digests from the
     effects scan), `checkpoint_seq_by_digest` (checkpoint digests).
   - **range deletes** — `transactions` / `effects` / `events` /
     `tx_metadata_by_seq` over `[tx_lo, tx_hi)`, `checkpoint_summary`
     / `checkpoint_contents` over `[ckpt_lo, ckpt_hi)`. `tx_hi` is the
     `network_total_transactions` of the chunk's highest checkpoint.
   - the new `PruningWatermarks` row.
   Commit, then `set_pruning_floor` advances the in-memory bitmap
   floor. Because the watermark row rides the same atomic batch as the
   deletes, a crash either loses the whole chunk (re-pruned) or
   commits it wholesale — there is no partial-delete-without-watermark
   state, and the deletes are idempotent.
4. **Bitmap eviction.** Once the floor *reaches its retention target*
   (the final catch-up tick), force a compaction over
   `transaction_bitmap` / `event_bitmap` so their compaction filters
   drop fully-pruned buckets promptly. Partial-advance ticks during a
   backlog drain skip the whole-CF compaction so it does not become the
   per-tick long pole; natural background compaction still applies the
   same filter opportunistically in the meantime. (RocksDB only fully
   guarantees the filter runs on *materialized* values, not raw merge
   operands, so a freshly-merged bucket may take an extra
   collapse-then-filter cycle — fine for a per-epoch pruner.)

The live-set-bounded indexes (`live_objects`, `object_by_owner`,
`object_by_type`, `balance`, `package_versions`) and the tiny
`epochs` CF are never pruned.

## Orchestration

`indexer/mod.rs` hosts the top-level `Indexer` type that wires the
pipelines into one `framework::Indexer<Store<RpcStoreSchema>>`
driven by a `sui_consistent_store::Synchronizer`. Two construction
paths:

- `Indexer::new(path, …)` opens the `Db` / `Store` internally.
  Typical for the standalone-binary path.
- `Indexer::from_store(store, …)` accepts an already-opened
  `Store`. Typical for the embedded-fullnode path where the
  fullnode shares the underlying `Db` with this indexer (for
  direct reads through `RpcStoreSchema`, and possibly its own
  raw-chain-data writes through a separate path).

Both constructors take an `IngestionClient` (built by the caller
via `IngestionClient::new(client_args)` for the standalone path
or `IngestionClient::from_trait(arc, metrics)` for the embedded
fullnode) plus an optional `BoxedStreamingClient`. The metrics
handle the client carries is reused by the `IngestionService` the
orchestrator builds internally, avoiding double-registration
against the prometheus registry.

`Indexer::add_pipelines(layer, committer)` registers every
pipeline that is `Some(_)` in the supplied `PipelineLayer` and no
others. The `Synchronizer`'s snapshot barrier only includes the
pipelines that were actually registered, so leaving the raw-chain-
data pipelines off does *not* stall snapshots. Each pipeline is
registered with `max_batch_checkpoints = 1` so the synchronizer's
one-checkpoint-per-batch contract is upheld regardless of any
default the handler trait might set.

`Indexer::run` installs the synchronizer's per-pipeline mpsc
queues onto the store, starts the framework indexer, and returns
a composed `Service` driving both for the lifetime of the
indexer.

`Indexer::from_store` also calls
`RpcStoreSchema::refresh_pruning_atomics` after opening the
store so the bitmap CFs' compaction filters resume against the
persisted `tx_seq` floor. The floor lives in a process-wide
`OnceLock<Arc<AtomicU64>>` (see `schema/pruning_watermark.rs`)
because the compaction filter is constructed inside
`Schema::cfs`, which has no access to per-instance state. Fine
for one-DB-per-process production; parallel tests in the same
binary share the floor.

### Configuration

`config.rs` exposes:

- `ServiceConfig` — top-level (`consistency`, `committer`,
  `pipeline`). Serializes from TOML via `sui-default-config`.
- `ConsistencyConfig` — `stride`, `buffer_size`. Threaded into
  `Synchronizer::new` at startup. Snapshot *retention* is not here:
  it is an open-time database property set via
  `DbOptions::snapshot_capacity` (the `sui-rpc-node` `[db]`
  `snapshot-capacity` knob), so the consistent-read window spans
  roughly `stride * snapshot_capacity` checkpoints.
- `PipelineLayer` — one `Option<CommitterLayer>` per pipeline.
  Constructors: `PipelineLayer::all()` (every pipeline enabled —
  standalone default) and `PipelineLayer::indexes_only()` (only
  the six derived-index pipelines — embedded-fullnode default).
- `PrunerConfig` — `Option` on `ServiceConfig` (absent disables
  pruning). Epoch-based `retention_epochs`, `interval_ms`,
  `max_chunk_checkpoints` (per-batch bound), and
  `max_checkpoints_per_tick` (per-tick bound). See "Pruning" below.
- `CommitterLayer` — per-pipeline overrides for
  `write_concurrency`, `collect_interval_ms`,
  `watermark_interval_ms`. `layer.finish(base)` folds an override
  onto a shared `CommitterConfig` default.

### Standalone entry point

`lib.rs::start_indexer(path, indexer_args, client_args,
db_options, ingestion_config, config, registry)` constructs the
`IngestionClient` and optional `GrpcStreamingClient` from
`ClientArgs`, builds the `Indexer`, calls `add_pipelines`, and
runs the result. This is the standalone binary's one-shot
helper; the embedded-fullnode path bypasses it and uses
`Indexer::from_store` directly.

### Required framework changes (landed)

Trait-object injection of the ingestion / streaming clients
required four small additive changes to
`sui-indexer-alt-framework`:

1. `IngestionClientTrait` is now `pub`.
2. `IngestionClient::from_trait(client, metrics)` wraps an
   arbitrary trait impl. (Renames the previously `pub(crate)`
   `new_impl`.)
3. `IngestionClient::metrics() -> &Arc<IngestionMetrics>` so peer
   services can reuse the same metrics handle.
4. `IngestionService::with_clients(ingestion_client, streaming,
   config, metrics)` bypasses `ClientArgs`-driven construction
   and takes the shared metrics Arc directly.
5. `Indexer::with_ingestion_service(store, indexer_args, service,
   metrics_prefix, registry)` accepts a pre-built
   `IngestionService`, bypassing `ClientArgs` at the indexer
   layer.
6. `IngestionService.streaming_client` is now
   `Option<Box<dyn CheckpointStreamingClient + Send>>` (aliased
   as `ingestion::BoxedStreamingClient`); the broadcaster drops
   its `S: CheckpointStreamingClient` generic. `latest_checkpoint_number`
   takes `&mut self` because the trait's methods require `&mut`;
   the streaming side is consulted once and the ingestion-client
   fallback is retried on transient errors (unchanged behaviour
   for ingestion).

## `sui-rpc-api` reader adapter

`reader/` hosts `RpcStoreReader<R>`, the adapter exposing
`RpcStoreSchema` through the trait stack `sui-rpc-api` consumes:
`ObjectStore`, `ReadStore`, `ChildObjectResolver`, `RpcStateReader`,
and `RpcIndexes`. The wrapper is generic over a
`sui_consistent_store::Reader`, so a single struct serves both tip
reads (`R = Db`) and point-in-time reads bound to a snapshot
(`R = Snapshot`). `RpcStoreReader::at_snapshot(&snap)` re-projects
a tip reader against a captured snapshot; the wrapper is `Clone`
so callers can hand it to `sui-rpc-api::StateReader::new(Arc::new(reader))`.

Module layout:

- `reader/object_store.rs` — `ObjectStore` impl: point lookups via
  `live_objects` → `objects`.
- `reader/read_store.rs` — `ReadStore` impl: committee, checkpoint
  headers / contents (by digest and seq), latest / highest /
  lowest checkpoint, transactions / effects / events /
  unchanged-loaded-objects / checkpoint-for-tx, all going through
  `tx_seq_by_digest` to translate digest → assigned tx_seq.
  `get_checkpoint_contents_by_digest` and
  `get_full_checkpoint_contents` are deliberately stubbed (return
  `None`); the latter is a state-sync path not on the rpc-api hot
  path, and the former needs a separate
  `CheckpointContentsDigest → seq` index that the store doesn't
  currently maintain.
- `reader/indexes.rs` — `RpcIndexes` impl. Covers `get_epoch_info`,
  `owned_objects_iter` (with optional `StructTag` filter and
  opaque cursor), `dynamic_field_iter` (prefix scan on
  `object_by_owner` with `(ObjectOwner, parent)`), `get_balance`
  / `balance_iter`, `package_versions_iter`,
  `get_highest_indexed_checkpoint_seq_number` (min across
  framework watermarks), `ledger_tx_seq_digest` / `_iter`, the two
  bitmap iterators (`transaction_bitmap_bucket_iter` /
  `event_bitmap_bucket_iter` over `(dim_key, bucket)` ranges,
  forward or reverse, with the raw `RoaringBitmap` bytes
  deserialized into `LedgerBitmapBucket`), and `get_coin_info`
  (discovers `CoinMetadata<T>` / `TreasuryCap<T>` /
  `RegulatedCoinMetadata<T>` via `object_by_type` prefix scans on
  the wrapper struct tags).
- `reader/child_resolver.rs` — `ChildObjectResolver` impl
  returning `Ok(None)` from both methods. This adapter is
  read-only and doesn't serve Move execution; execution-time
  child-object resolution stays on the validator perpetual store.
- `reader/layout.rs` — Move type-layout wiring. Builds a
  `PackageStoreOverObjects` (`BackingPackageStore` impl over
  `live_objects → objects`), wraps it in
  `OverlayBackingPackageStore` honouring the caller's overlay,
  resolves the live `ProtocolConfig` from the latest epoch's
  `SuiSystemState` (protocol version) and the framework
  `__chain_id` CF (chain id), and calls `sui_execution::executor`
  → `type_layout_resolver().get_annotated_layout()`. Returns
  `Ok(None)` when either side of the protocol-config triple is
  missing (no checkpoints observed, no chain id recorded).
- `reader/state_reader.rs` — top-level `RpcStateReader` rollup:
  `get_lowest_available_checkpoint_objects` (pruning watermark),
  `get_chain_identifier` (framework `__chain_id` CF), `indexes()`
  → `Some(self)`, `get_struct_layout_with_overlay` delegating to
  the layout shim.

## Validated end to end

The full lifecycle was exercised against mainnet this session:
formal-snapshot restore (epoch 1147 snapshot, landing the chain at
the 1147→1148 boundary) → `serve` (query the restored state with no
indexer) → tip indexing (backfill from the checkpoint store + gRPC
stream handoff at the head). The restored `serve` reads matched the
live fullnode exactly for owned objects and (non-zero) balances, and
after catch-up the node tracks the live tip to within a checkpoint.
The one observed divergence is the zero-balance handling noted below.

## Deferred work

In rough priority order:

- **Pruning follow-ups** — the pruner (see "Pruning") is landed, but:
  (a) chunks are bounded only by checkpoint count, not by transaction
  count, so a chunk during a high-throughput epoch can stage a large
  batch — thread a `max_chunk_transactions` bound through
  `PrunerConfig` if that proves heavy (deliberately deferred: the
  per-tick budget keeps the granularity at checkpoint bounds); (b) no
  real-chain end-to-end test exercises the pruner against a
  multi-epoch stream (unit + `prune_chunk` + `prune_once` per-tick
  integration coverage exists); (c) bitmap eviction of freshly-merged
  buckets may lag one compaction cycle (see "Pruning"). The
  one-blocking-pass concern is resolved: `prune_once` now advances at
  most `max_checkpoints_per_tick` checkpoints per tick.
- **Embedded-fullnode ingestion / streaming clients** —
  `Indexer::from_store` accepts an `IngestionClient` (built via
  `IngestionClient::from_trait`) and an optional
  `BoxedStreamingClient`, but no impls exist yet that source
  checkpoints from inside `sui-node` / `sui-core`. Need an
  `IngestionClientTrait` impl over the validator's local
  checkpoint store and a `CheckpointStreamingClient` impl over the
  node's existing tip broadcast. Both live in `sui-core` /
  `sui-node`, not this crate; the rpc-store side is ready to
  consume them.
- **`ListBalances` zero-balance parity** — the `balance` CF's
  compaction filter drops rows where both components are zero, so
  `ListBalances` omits coin types a holder owns only as zero-value
  coin objects. The validator's rpc-index instead reports them
  with `balance: 0`. Verified against the live fullnode (`0x0`,
  `0x2a5f…`): every non-zero balance matches exactly; the only
  divergence is these dropped zeros. Decide whether to match the
  validator (keep / report the zero rows) or keep dropping them,
  and adjust the filter / `get_balance` path accordingly.
- **Configurable backpressure** — the sequential pipeline's
  `min_eager_rows` / `max_pending_rows`, the channel depths
  (`processor_channel_size`, `pipeline_depth`), and the ingestion
  `subscriber_channel_size` are all left at framework defaults,
  hardcoded via `..Default::default()` in
  `Indexer::add_pipelines`; `CommitterLayer` surfaces only
  `write_concurrency` / `collect_interval_ms` /
  `watermark_interval_ms`. Thread the remaining `SequentialConfig`
  knobs through `CommitterLayer` so operators can tune per-pipeline
  buffering / flush behavior. (`max_batch_checkpoints` must stay
  pinned at `1` — the synchronizer requires exactly one checkpoint
  per batch. `WARN_PENDING_WATERMARKS` is a framework const, not a
  `SequentialConfig` field, so exposing it would need a framework
  change.)
- **Epoch-start seed runs only at restore finalize** —
  `seed_current_epoch_start` fires inside
  `floor_unrestored_pipelines`, so a database restored before that
  change (or via a path that skips the `objects` CF) has no start
  record for the landing epoch until the next epoch boundary. No
  retroactive backfill / in-place repair command exists.
- **Concurrent pipelines with sequential gating** — some pipelines
  (raw chain data) could run as concurrent rather than sequential
  pipelines; the synchronizer would need to know which to gate on
  for stride-boundary snapshots. A future `sui-consistent-store`
  change; not implemented.
- **Cross-shard snapshot consistency for perpetual-store source** —
  each shard's iterator is snapshot-consistent for the shard's
  lifetime, but different shards' snapshots are taken at different
  wall-clock times. Today's fullnode restore is blocking so this
  can't skew; an online restore would need one rocksdb snapshot
  pre-allocated and shared across all shard iterators.
- **End-to-end tests** — integration coverage landed this session:
  a Simulacrum-backed `LocalCluster` driving the rpc-api gRPC
  surface, a `serve`-mode startup test, the per-CF stats-collector
  test, the epoch-start-seed test, and the restore-driver resume +
  `restore_shards_done` test. Still missing: a restore test against
  the formal-snapshot source's *real* file layout, and an automated
  full real-chain-stream assertion (that flow was validated
  manually this session — see "Validated end to end").

## File layout

```
sui-rpc-store/
├── Cargo.toml
├── codegen.rs                       # cargo nightly script for proto codegen
├── proto/sui/rpc_store/v1alpha/
│   └── types.proto                  # all value protos
└── src/
    ├── lib.rs                       # re-exports + start_indexer entry point
    ├── config.rs                    # ServiceConfig, PipelineLayer,
    │                                # ConsistencyConfig, CommitterLayer
    ├── proto/
    │   ├── mod.rs                   # re-exports generated types
    │   └── generated/               # output of codegen.rs
    ├── schema/
    │   ├── mod.rs                   # RpcStoreSchema struct + Schema impl,
    │   │                            # default_rocksdb_config()
    │   ├── keys.rs                  # shared keys (U64Be, U64Varint, UnitKey,
    │   │                            #  StructTagKey, TypeTagKey, …)
    │   ├── type_filter.rs           # TypeFilter for prefix queries
    │   ├── pruning_watermark.rs     # shared Arc<AtomicU64> for bitmap filters
    │   └── <cf_name>.rs             # one module per CF, with NAME, Key,
    │                                # Value, options(), store helpers, and
    │                                # impl RpcStoreSchema read methods
    ├── indexer/
    │   ├── mod.rs                   # Schema/Store aliases, tx_seq_at,
    │   │                            # checkpoint_{input,output}_objects,
    │   │                            # Indexer orchestrator
    │   ├── restore.rs               # restore_indexes() entry point;
    │   │                            # registers the five Restore impls
    │   │                            # on a RestoreDriver
    │   ├── pruner.rs                # standalone retention-floor pruner
    │   │                            # (epoch-based; point + range
    │   │                            # deletes + bitmap compaction)
    │   └── <pipeline_name>.rs       # one module per pipeline:
    │                                # Processor + sequential::Handler,
    │                                # plus Restore impls where applicable
    └── reader/
        ├── mod.rs                   # RpcStoreReader<R> skeleton +
        │                            # at_snapshot
        ├── object_store.rs          # impl ObjectStore
        ├── read_store.rs            # impl ReadStore
        ├── indexes.rs               # impl RpcIndexes
        ├── child_resolver.rs        # impl ChildObjectResolver (Ok(None))
        ├── layout.rs                # Move type-layout wiring +
        │                            # PackageStoreOverObjects
        ├── state_reader.rs          # impl RpcStateReader rollup
        └── integration_test.rs      # trait-surface integration tests
```
