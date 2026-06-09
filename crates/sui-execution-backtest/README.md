<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# sui-execution-backtest

Backtests the execution layer against historical mainnet data: it re-executes past transactions
under the **current** execution rules and reports where the recomputed result diverges from what was
recorded on chain. Useful for measuring the behavioral impact of an execution/protocol change before
it ships (e.g. a VM, gas, or linkage change).

For a range of epochs it:

1. Resolves each epoch's checkpoint range + protocol version from a fullnode (gRPC `GetEpoch`).
2. Streams the epoch's checkpoints (via `sui-indexer-alt-framework`'s ingestion client) through a
   two-stage pipeline: a prefetch stage fetches + indexes up to `--concurrency` checkpoints at once
   into a bounded buffer, and an execute stage drains it, re-executing up to `--execute-concurrency`
   transactions concurrently on blocking workers. Decoupling fetch from execute keeps the cores fed
   regardless of fetch latency.
3. For every programmable transaction whose on-chain status matches `--status` (default `all`),
   fully re-executes it against reconstructed checkpoint state via the `sui-execution` Executor.
4. Records a **divergence** for any transaction whose recomputed success/failure status disagrees
   with its on-chain status, writing it to an NDJSON file as it is found (flushed per progress tick,
   so partial results survive an interrupted run).

The strict baseline — "succeeded on chain, now errors" — is `--status success`, so any divergence is
a clear regression. With `--status all`/`failed` the on-chain status is recorded per record so the
differential can be applied downstream.

Divergence direction is recoverable from each record: a recomputed error (on-chain succeeded) has a
non-null `recomputed_error`; a recomputed success (on-chain failed) has `recomputed_error: null`.

## Run

```bash
cargo run --release -p sui-execution-backtest -- \
  --remote-store-url https://checkpoints.mainnet.sui.io \
  --fullnode-url https://fullnode.mainnet.sui.io:443 \
  --start-epoch 1146 \
  --end-epoch 1147 \
  --concurrency 64 \
  --execute-concurrency 24 \
  --prefetch-depth 128 \
  --cache ./.package-cache \
  --output ./divergences.ndjson
```

- **Checkpoint source.** Prefer a remote object store (`--remote-store-url
  https://checkpoints.<network>.sui.io`) — it is the fast archival path. A fullnode for epoch +
  package resolution must then be supplied separately with `--fullnode-url`. Alternatively a single
  `--rpc-api-url` fullnode can serve as both checkpoint source and resolver (slower; see the
  rate-limit caveat below).
- `--start-epoch` / `--end-epoch` select the inclusive epoch range.
- `--max-checkpoints-per-epoch N` caps each epoch at its first `N` checkpoints (for bounded
  samples). Omit it to backtest whole epochs — note a mainnet epoch is ~300–390k checkpoints.
- `--status {success,failed,all}` (default `all`) selects which on-chain statuses to re-execute.
- `--concurrency` (default 32) is the **fetch** width: checkpoints fetched + indexed concurrently by
  the prefetch stage.
- `--execute-concurrency` (default ~2× cores) is the **CPU** width: transactions executed
  concurrently on blocking workers.
- `--prefetch-depth` (default `--concurrency`) is the buffer depth between the fetch and execute
  stages; a larger buffer absorbs fetch-latency bursts at the cost of memory (each buffered
  checkpoint holds its object set).
- `--cache` points at an on-disk package cache directory (speeds up re-scans).

Each output line looks like:

```json
{"digest":"…","checkpoint":282446861,"epoch":1147,"original_status":"success","original_failure":null,"recomputed_error":"ExecutionError { … kind: InvalidLinkage, source: Some(\"…conflicting resolutions…\") }"}
```

The `backtest complete` log line reports the run totals: `total_checked`, `total_divergences`,
`total_reconstruction_errors`, `total_executed`, `total_cancellation_excluded`, and the
skip/count categories below, plus `tx_per_s` / `cp_per_s`.

## Performance & tuning

Fetch and execute are separate pipeline stages, so fetch latency can't starve the cores and a deep
`--prefetch-depth` only trades memory for burst tolerance. Tuning guidance (measured on a 20-core
machine, warm `--cache`, remote object store as the checkpoint source):

- Throughput plateaus around **~2,200 tx/s / ~170 checkpoints/s**. The ceiling
  is the fetch path / memory bandwidth, **not** CPU — a single process
  saturates at around 10–12× effective parallelism regardless of the knobs.
- `--execute-concurrency` has a bound around **16–24**; below it you lose throughput, above it does
  nothing.
- `--concurrency` is best around **48–64**; raising it further slightly *hurts*.
- Sharding one box into multiple processes does **not** raise aggregate throughput (the bottleneck
  is shared); to go faster, use a closer/faster checkpoint source.

## How execution context is reconstructed

Per transaction the tool builds a read-only `BackingStore` from:

- **Objects**: the checkpoint's object set (input + output + unchanged-loaded-runtime objects).
  Shared objects are served at their per-transaction version (from the effects' input consensus
  objects), and dynamic-field child reads are tombstone-aware (within-checkpoint deletions are
  honored).
- **Packages**: looked up in that object set first, then a process-wide cache (in-memory behind an
  `RwLock`, layered over the optional on-disk `--cache` dir), then a gRPC fetch from the fullnode
  (with retry + exponential backoff on rate-limit / transient errors).

Execution is **metered** with the transaction's own budget/price (gasless txns are metered at the
epoch RGP with the gasless compute cap, mirroring `sui-transaction-checks`). Because `BackingStore`
is synchronous, each transaction runs on a blocking worker (`spawn_blocking`).

Coin-reservation (address-balance) inputs — synthetic "fake coin" object refs that encode a
withdrawal in their digest — are rewritten back into `FundsWithdrawal` args by re-deriving the
balance type from the reservation id (no extra fetch). Reservations whose type can't be identified
are counted in `coin_reservation_skipped` and skipped.

## Caveats

- **Single-transaction replay can't model scheduling.** Transactions cancelled before execution by
  consensus-layer congestion / randomness control (`ExecutionCancelledDueToSharedObjectCongestion`,
  `ExecutionCancelledDueToRandomnessUnavailable`) never ran on chain, so they "succeed" here. These
  are detected from the on-chain effects and counted under `cancellation_excluded` rather than
  reported as divergences.
- **Public-node rate limiting.** Package fetches (and, if `--rpc-api-url` is the checkpoint source,
  checkpoint fetches) go to the fullnode; `fullnode.mainnet.sui.io` returns HTTP 429 under
  concurrent load. The package fetcher retries with backoff so results aren't corrupted, but high
  concurrency against a shared node mostly sleeps in backoff. Prefer `--remote-store-url` for
  checkpoints plus a dedicated/archival fullnode; against the public fullnode keep `--concurrency`
  low (≈4). Public nodes also **prune old epochs** — only recent epochs are available there.
- Only `ProgrammableTransaction`s are considered; system/consensus transactions are out of scope.

## Analysis

The backtest itself is change-agnostic — it just emits divergence records. Analysis specific to a
particular change lives outside this crate.
