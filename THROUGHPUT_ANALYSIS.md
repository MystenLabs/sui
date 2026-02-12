# KV-Store Indexer Throughput Bottleneck Analysis

## Problem Statement

Running on a 16-core (12 allocated) machine with FANOUT=100 for all pipelines, only achieving ~30% CPU utilization. Not hitting adaptive ingestion limits, DB write limits, or any CPU/bandwidth limits on the host. There is a software bottleneck preventing the system from scaling.

## Current Config

| Knob | Value | Where Set |
|------|-------|-----------|
| `FANOUT` | 100 | Rust code (all 7 pipelines override in `BigTableProcessor`) |
| `PROCESSOR_CHANNEL_SIZE` | 1000 | Env var via Pulumi (`Pulumi.testnet-loadtest.yaml:30`) |
| `COMMITTER_CHANNEL_SIZE` | 100 | Env var via Pulumi (`Pulumi.testnet-loadtest.yaml:31`) |
| `WATERMARK_CHANNEL_SIZE` | 100 | Env var via Pulumi (`Pulumi.testnet-loadtest.yaml:32`) |
| `checkpoint_buffer_size` | 5000 | TOML (`config/testnet-loadtest.toml:3`) |
| `write_concurrency` | Vegas(initial=1, max=1000, smoothing=1.0, beta_factor=30.0, probe_multiplier=30) | TOML (`config/testnet-loadtest.toml:6`) |
| `ingest_concurrency` | AIMD(initial=10, max=100000, successes_per_increase=10) | TOML (`config/testnet-loadtest.toml:2`) |
| `max_mutations` | 100 | Pulumi yaml (`Pulumi.testnet-loadtest.yaml:29`) |
| `MAX_PENDING_ROWS` | 5000 | Framework default, not overridden (`concurrent/mod.rs:78`) |
| `MIN_EAGER_ROWS` | 50 | BigTableProcessor default (`handler.rs:35`) |
| `collect_interval_ms` | 500 | CommitterConfig default (not in TOML) |
| `watermark_interval_ms` | 500 | TOML (`config/testnet-loadtest.toml:7`) |
| num_cpus | 12 | Pulumi yaml (`Pulumi.testnet-loadtest.yaml:33`) |
| Pipelines | 6-7 concurrent | `lib.rs:224-265` |

## Architecture: The Full Data Flow

```
Ingestion (AIMD adaptive, up to 100k concurrent fetches)
    │  broadcast via try_join_all (broadcaster.rs:476-477)
    │  ALL subscriber channels must have capacity or ALL block
    │
    ├──► Subscriber ch [5000] ──► Processor (FANOUT=100) ──► [1000] ──► Collector ──► [100] ──► Committer (Vegas) ──► [100] ──► Watermark
    ├──► Subscriber ch [5000] ──► Processor (FANOUT=100) ──► [1000] ──► Collector ──► [100] ──► Committer (Vegas) ──► [100] ──► Watermark
    ├──► ... (x6-7 pipelines, each has its own independent channels + Vegas limiter)
    │
    ◄── Regulation: ingest_hi = min(all pipeline watermarks) + checkpoint_buffer_size(5000)
        (broadcaster.rs:148-151, updated via commit_hi_rx, feedback latency = watermark_interval_ms)
```

Pipelines: Checkpoints, CheckpointsByDigest, Transactions, Objects, EpochStart, EpochEnd, (optional EpochLegacy).

All wrap `BigTableProcessor` via `BigTableHandler<P>` (`handler.rs`), sharing the same `concurrent::Handler` batching/commit logic.

## Stage-by-Stage Breakdown

### Stage 1: Ingestion (broadcaster.rs)

- `ingest_and_broadcast_range()` uses `try_for_each_spawned_adaptive_with_retry` with the AIMD limiter
- Fetches checkpoints concurrently, then broadcasts via `send_checkpoint()` which does `try_join_all` across ALL subscriber senders
- **Key**: if ANY subscriber channel is full, the send blocks for ALL pipelines (broadcaster.rs:476-477)
- Regulation: `backpressured_checkpoint_stream` (broadcaster.rs:196-217) gates yielding checkpoints until `cp < ingest_hi`, where `ingest_hi = min(all pipeline commit_hi) + buffer_size`
- Watermark feedback latency: `watermark_interval_ms` (500ms default)

### Stage 2: Processor (processor.rs:66-177)

- Uses `try_for_each_spawned(P::FANOUT, ...)` from `sui-futures/src/stream.rs`
- **Critical detail**: each spawned task does BOTH processing AND sending in a single future (processor.rs:89-157):
  ```rust
  let values = processor.process(&checkpoint).await?;  // CPU work
  tx.send(IndexedCheckpoint::new(..., values)).await?;  // MAY BLOCK if channel full
  ```
- Permits (stream.rs:63,77,95): a permit is consumed when a task spawns and returned ONLY when the entire future completes (including the send). Tasks blocked on `tx.send()` hold their permit.
- Channel: `mpsc::channel(processor_channel_size(H::FANOUT))` where `processor_channel_size` reads env `PROCESSOR_CHANNEL_SIZE` or defaults to `FANOUT + 5` (mod.rs:36-45)

### Stage 3: Collector (concurrent/collector.rs:74-226)

- **Single async task** per pipeline. Runs a `tokio::select!` loop with two branches:
  1. `poll.tick()` (collector.rs:104): creates ONE batch from pending data, sends to committer channel. If `pending_rows > 0` after sending, calls `poll.reset_immediately()`.
  2. `rx.recv()` (collector.rs:189): receives one `IndexedCheckpoint` from processor channel. **Guarded by `pending_rows < H::MAX_PENDING_ROWS`** (default 5000). If `pending_rows >= MIN_EAGER_ROWS` (50), calls `poll.reset_immediately()`.
- Batch creation calls `handler.batch()` (handler.rs:90-107) which fills a `BigTableBatch` until `total_mutations == max_mutations` (100), then returns `BatchStatus::Ready`.
- **MAX_PENDING_ROWS = 5000** is the key backpressure point. With ~300 objects/checkpoint (Objects pipeline), the collector can only buffer ~17 checkpoints before stopping recv.
- Channel out: `mpsc::channel(committer_channel_size())` which reads env `COMMITTER_CHANNEL_SIZE` or defaults to 10 (mod.rs:49-57)

### Stage 4: Committer (concurrent/committer.rs:41-282)

- Uses `try_for_each_spawned_adaptive_with_retry` from `sui-concurrency-limiter/src/stream.rs:175-272`
- Vegas limiter gates concurrency: `can_spawn = !draining && active < current_limit` (stream.rs:201)
- Each spawned task: acquires token (`limiter.acquire()`), does DB write (`handler.commit()`), records sample (`token.record_sample(Outcome::Success)`), then sends watermark (unmeasured work)
- Channel out: `mpsc::channel(watermark_channel_size())` which reads env `WATERMARK_CHANNEL_SIZE` or defaults to `num_cpus + 5` (mod.rs:62-70)

### Stage 5: Watermark (concurrent/commit_watermark.rs)

- Receives watermark parts from committer, advances contiguous watermark
- Writes to DB every `watermark_interval_ms` (500ms)
- Sends `commit_hi` back to broadcaster via unbounded channel
- **Contiguous progression requirement**: a single gap in checkpoint sequence blocks watermark advancement

## The Backpressure Cascade (Why 30% CPU)

The chain that causes low CPU utilization:

1. **Committer throughput** is the rate at which batches are written to BigTable: `vegas_limit * (1 / avg_write_latency)` batches/sec. Each batch is only `max_mutations=100` rows.

2. When committer can't keep up, **collector->committer channel (100 slots) fills**. Collector blocks on `tx.send(batched_rows).await` at collector.rs:169.

3. When collector is blocked sending, it **can't receive** from processor channel. Also, `MAX_PENDING_ROWS=5000` independently limits the collector to ~17 checkpoints of buffer (at ~300 objects/checkpoint for Objects pipeline). When hit, the recv guard `if pending_rows < H::MAX_PENDING_ROWS` at collector.rs:189 disables receiving.

4. **Processor->collector channel (1000 slots) fills up** because the collector is blocked/throttled.

5. **Processor tasks block on `tx.send()`** (processor.rs:143-151) while holding their FANOUT permits. Of 100 permits, ~95+ are held by tasks waiting to send. Only ~3-5 tasks are actively doing CPU work at any moment.

6. **Result**: ~30% of 12 CPUs = 3.6 cores active, matching a few active processor tasks across 6 pipelines.

**Key observation**: the user reports the processor channel IS full at 1000. This confirms the downstream is the bottleneck. The processor produces faster than the collector+committer can drain.

**Key observation**: the user reports NOT hitting committer concurrency limits (Vegas not maxed). This means the committer has CAPACITY but isn't being FED fast enough. The bottleneck shifts upstream to the collector.

## Primary Bottleneck: MAX_PENDING_ROWS = 5000

Given that committer concurrency limits are NOT being hit, the bottleneck is the **collector's ability to feed the committer**. The collector is throttled by `MAX_PENDING_ROWS=5000`:

- Objects pipeline: ~300 objects/checkpoint -> collector can hold ~17 checkpoints worth of data
- Collector creates batches of `max_mutations=100` -> ~3 batches per checkpoint -> ~50 batches to drain 5000 rows
- While draining (creating batches), the collector can't receive (guard disables recv)
- This creates a stop-start pattern: receive 17 checkpoints, batch 50 times, repeat
- The committer channel (100 slots) can buffer 100 batches, but the collector fills it in bursts then starves it while receiving

The result: the committer has capacity (Vegas limit is high enough) but its input channel alternates between "burst of batches" and "empty" as the collector cycles between receiving and batching. The committer's inflight drops during the empty periods, and the **Vegas app-limiting guard** (`lib.rs:614`: `inflight * 2 < estimated_limit -> skip adjustment`) may prevent Vegas from growing further because inflight is low relative to the limit.

## Secondary Bottleneck: max_mutations = 100

Each batch is only 100 mutations (BigTable hard limit is 100,000). This means:
- Each checkpoint with ~300 objects requires 3 separate BigTable round-trips
- Each round-trip has fixed overhead (gRPC, serialization, network)
- Even at Vegas limit=100: `100 concurrent * 100 mutations / 20ms = 500k mutations/sec = ~1,667 checkpoints/sec` at 300 mutations each
- Increasing to 10,000 would mean 1 batch per checkpoint instead of 3, dramatically reducing overhead

## Tertiary Factors

### Collector is single-threaded
- Creates one batch per `select!` iteration
- Fast (microseconds per batch when channels have capacity), so not the primary bottleneck
- But it IS a serial funnel between processor (100x parallel) and committer (Nx parallel)

### Regulation feedback loop latency
- Watermarks update every 500ms (watermark_interval_ms)
- Regulation: `ingest_hi = min(all watermarks) + 5000`
- 500ms feedback delay creates sawtooth ingestion pattern
- Slowest pipeline (Objects) holds back ALL pipelines

### Vegas ramp-up behavior
- With `beta_factor=30.0`: jumps from 1->31 on first good sample (lib.rs:625-626: `new_limit = limit + beta` where beta=ceil(30*log10_root(1))=30)
- After initial jump, growth depends on latency stability. With any variance, queue_size lands between alpha(3) and beta(30), causing "hold steady" (lib.rs:631-636)
- Moderate growth path (+log10_root per sample) is slow but steady
- Probes (every `jitter * 30 * limit` samples) reset rtt_noload, allowing fresh growth

## Vegas Algorithm Details (sui-concurrency-limiter/src/lib.rs:568-653)

```
On each Success sample:
1. If rtt_noload == 0 or rtt < rtt_noload: calibrate, skip
2. If probe triggered (every ~jitter * probe_multiplier * limit samples): reset rtt_noload, skip
3. App-limiting guard: if inflight * 2 < limit: skip (system underutilized)
4. queue_size = ceil(limit * (1 - rtt_noload / rtt))
5. thresholds: alpha = ceil(alpha_factor * log10_root(limit)), beta = ceil(beta_factor * log10_root(limit))
6. if queue_size <= log10_root(limit): limit += beta  (aggressive growth)
   if queue_size < alpha: limit += log10_root(limit)  (moderate growth)
   if queue_size > beta: limit -= log10_root(limit)   (decrease)
   else: hold steady (no smoothing applied)
7. Clamp to [min_limit, max_limit], then smooth
```

With user's config (beta_factor=30, alpha_factor=3 default):
- At limit=1: log10_root=1, alpha=3, beta=30 -> if queue_size<=1: jump by +30
- At limit=31: same thresholds (log10_root still 1) -> growth depends on queue_size
- At limit=100: log10_root=2, alpha=6, beta=60 -> bigger jumps possible

## Recommended Fixes

### 1. Increase MAX_PENDING_ROWS (likely highest impact given symptoms)
The user is NOT hitting committer limits, meaning the committer has capacity but the collector can't feed it fast enough. Override `MAX_PENDING_ROWS` in the BigTableHandler:

**File**: `crates/sui-kvstore/src/handlers/handler.rs`, add to `impl<P> Handler for BigTableHandler<P>`:
```rust
const MAX_PENDING_ROWS: usize = 50_000;  // was 5000 default
```

This lets the collector buffer ~167 checkpoints (at 300 objects each) instead of ~17, keeping the committer fed more consistently.

### 2. Increase max_mutations
**File**: `Pulumi.testnet-loadtest.yaml:29`
```yaml
max_mutations: 10000  # was 100
```
Reduces BigTable round-trips per checkpoint by ~100x. Fewer, larger batches = less overhead.

### 3. Reduce collect_interval_ms
Add to TOML config:
```toml
[committer]
collect-interval-ms = 100  # default is 500
```
Reduces delay when pending data is below MIN_EAGER_ROWS threshold.

### 4. Reduce watermark_interval_ms
```toml
[committer]
watermark-interval-ms = 100  # was 500
```
Faster regulation feedback loop, reduces sawtooth ingestion pattern.

### 5. Verify Vegas behavior via metrics
Check these metrics per pipeline:
- `committer_write_concurrency`: current Vegas limit
- `committer_write_peak_inflight`: are Vegas slots being used?
- `committer_write_peak_concurrency`: peak Vegas limit
- `processor_peak_channel_utilization`: is processor channel full?
- `collector_peak_channel_utilization`: is collector->committer channel full?
- `committer_commit_latency`: BigTable write latency

If Vegas is stuck at a low limit due to the app-limiting guard (inflight low because collector can't feed fast enough), consider testing with `Fixed { limit: 200 }` to isolate whether Vegas is the issue.

## Key Code Locations

| Component | File | Key Lines |
|-----------|------|-----------|
| Channel size functions | `crates/sui-indexer-alt-framework/src/pipeline/mod.rs` | 23-70 |
| Processor task | `crates/sui-indexer-alt-framework/src/pipeline/processor.rs` | 66-177 |
| try_for_each_spawned (permit logic) | `crates/sui-futures/src/stream.rs` | 46-126 |
| Concurrent pipeline setup | `crates/sui-indexer-alt-framework/src/pipeline/concurrent/mod.rs` | 225-282 |
| Handler trait (MAX_PENDING_ROWS) | `crates/sui-indexer-alt-framework/src/pipeline/concurrent/mod.rs` | 69-116 |
| Collector task | `crates/sui-indexer-alt-framework/src/pipeline/concurrent/collector.rs` | 74-226 |
| Committer task | `crates/sui-indexer-alt-framework/src/pipeline/concurrent/committer.rs` | 41-282 |
| Adaptive stream (Vegas gating) | `crates/sui-concurrency-limiter/src/stream.rs` | 175-272 |
| Vegas algorithm | `crates/sui-concurrency-limiter/src/lib.rs` | 568-653 |
| Broadcaster / regulation | `crates/sui-indexer-alt-framework/src/ingestion/broadcaster.rs` | 48-192 |
| BigTableHandler batch/commit | `crates/sui-kvstore/src/handlers/handler.rs` | 80-142 |
| BigTableProcessor (FANOUT overrides) | `crates/sui-kvstore/src/handlers/objects.rs` | 41-44 |
| Pipeline registration | `crates/sui-kvstore/src/lib.rs` | 224-265 |
| Pulumi config -> env vars | `sui-operations/pulumi/services/sui-kvstore/resources/kubernetes/kubernetes.go` | 239-254 |
| Pulumi config struct | `sui-operations/pulumi/services/sui-kvstore/config/config.go` | 35-39 |
