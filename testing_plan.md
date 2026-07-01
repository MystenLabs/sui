<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# v2alpha gRPC List APIs — Testing Plan

Status: **design / planning** (no implementation yet). This document captures the
goals, decisions, and open questions from the initial design discussion so the
work can be picked up later.

> **Read the "v0 milestone" section (immediately below §1) first.** Most of this
> doc is the full vision (CI, distribution, regression tracking, warehouse oracle).
> The **actual near-term goal is v0** — a manually-run test, no CI, whose only job
> is to clear the bar for announcing these APIs to the community. The numbered
> sections (§2–§6) are the deep design v0 draws from; they are v1+ and are
> documented now so v0 is built in the right direction, not to be done now.

## 1. Goal & scope

We are adding **load/performance** and **correctness** testing for the new
`sui.rpc.v2alpha.LedgerService` streaming List APIs before broadcasting them to
the Sui ecosystem and encouraging builders to adopt them:

- `ListCheckpoints`
- `ListTransactions`
- `ListEvents`

These are bitmap-index-backed, filtered, cursor-paginated, **server-streaming**
RPCs. Today they have **no real load/scale testing and no over-the-wire
integration testing at scale** — only Rust build-time unit/e2e tests.

**Priority: performance first.** Correctness is already reasonably covered at the
unit/e2e level (see §3); the things that genuinely can't be proven by a
`cargo test` — scale, real-data behavior, cross-backend agreement — are the gap,
and those need the new harness. **Out of scope:** transaction execution /
simulation load (write path).

### The two backends (one wire contract)

| | `sui-kv-rpc` | `sui-rpc-api` ledger_service |
|---|---|---|
| Store | BigTable (remote) | local RocksDB reader |
| Concurrency | async stream combinators | chunked `spawn_blocking` state machine |
| Role | KV-RPC / archival service | in-process on a fullnode w/ ledger history |

Both implement the **same wire contract** and are interchangeable to a client.
Shared contract code: `crates/sui-rpc-api/src/ledger_history/` (`query_options`,
`watermark`, `filter`); proto in the pinned `sui-rust-sdk` rev. Because they share
the interface, **fullnode and archival can share the same test harness** — only
their scalability expectations (and bottleneck profiles) differ. The fullnode
path is CPU/blocking-pool/disk-bound on narrow recent ranges; the kv-rpc path is
BigTable-network-bound on wide historical ranges.

### Key contract concepts (for test design)

- **Cursors are canonical ledger positions**, not ordering-relative. `after` =
  exclusive lower bound, `before` = exclusive upper bound, regardless of
  ordering. Opaque BCS token `{query_type, kind, checkpoint, position}`.
- **Watermarks** carry a resume cursor + exactly one of `checkpoint_hi`
  (ascending) / `checkpoint_lo` (descending) = the inclusive checkpoint fully
  covered. `list_checkpoints` dedupes cp (boundary can include it);
  `list_transactions`/`list_events` scan within a checkpoint (boundary excludes
  it, `C∓1`).
- **`QueryEndReason`**: `ITEM_LIMIT`, `SCAN_LIMIT` (bitmap bucket budget
  exhausted), `CHECKPOINT_BOUND`, `CURSOR_BOUND`, `LEDGER_TIP`. Only `LEDGER_TIP`
  / `CHECKPOINT_BOUND` are "natural completion."
- **Filtered queries** evaluate a DNF (`OR` of terms, each an `AND` of signed
  literals) over a roaring-bitmap inverted index, bounded by a per-request
  **scan budget** measured in buckets. The **degenerate worst case** = two
  individually-dense dimensions with empty intersection → every bucket walked,
  ANDed to zero, `SCAN_LIMIT` every page.

## v0 milestone — manual test to unblock the announcement (START HERE)

**The entire near-term goal.** Not CI, not distributed, not regression-tracked,
not a warehouse oracle. A test **I run by hand** that produces enough evidence to
confidently announce `ListCheckpoints` / `ListTransactions` / `ListEvents` to the
community. If it answers the three questions below, v0 is done.

### What "confident enough to announce" means — three questions

1. **Capacity (rough):** roughly how much can **one RPC replica** take on a
   realistic query mix before the knee (goodput stops tracking offered load)?
   A single ballpark number per backend is enough to set expectations and guide
   launch provisioning — not a tracked trend, just "order of hundreds vs
   thousands of streams/sec."
2. **Abuse resistance:** does a **flood of degenerate, budget-burning queries**
   (the dense∧dense empty-intersection shape, §1 / §2.3) degrade *gracefully* —
   load-shed, stay up, keep serving cheap point-reads — rather than crash, OOM, or
   starve the replica? This is the one I'd be most embarrassed to get wrong post-
   announcement.
3. **Correctness sanity:** over the **real filter shapes on a recent window**, does
   the **cross-backend differential** (kv-rpc vs fullnode, same query, diffed
   streams — §3) agree? This is the cheapest way to catch a silently broken or
   under-populated index before builders do. Unit tests cover the synthetic case;
   this covers real data.

If all three are green, the APIs are safe to broadcast. Performance *trend*
tracking, distribution, and CI are all about keeping them safe **over time** — a
v1 concern, explicitly out of v0.

### Workflow — corpus-first, correctness before load

One library, built once, used twice:
1. **Build the test-case set** from Snowflake (snow CLI) covering the full
   scenario range (§4.6) — saved as JSONL.
2. **Verify correctness** against it: cross-backend differential on the shared
   set + warehouse spot-check on the archival-only set (§3).
3. **Replay the same set as load** until the breaking point — a sanity check that
   it degrades gracefully (questions 1 & 2), not a tracked trend.

All manual and one-off for now; **matured with the team over the coming weeks**
into the §2–§6 vision (CI, distribution, regression tracking).

### Minimum harness (deliberately small)

- **One load generator process** — *not* k6-operator, *not* multi-pod. Stock k6 on
  a single beefy box, or the standalone Rust probe (`scan-history-bench`'s proven
  inner loop). The **streaming spike (§2.5) still gates which one** — do it first;
  it's a half-day and decides the whole tool. A single generator that can saturate
  a single replica is all v0 needs; cross-pod aggregation is a scale problem we
  don't have yet.
- **Point at the existing testbed** — the mainnet-backfilled kv-rpc
  (`kv-rpc-bitmap-scan-testbed` memory). **No new Pulumi / archival-fullnode
  buildout for v0.** The differential's fullnode side can be a recent-window
  fullnode (its natural range), or deferred to v1 if standing one up is friction —
  in which case v0 correctness leans on the warehouse spot-check + existing e2e
  tests, and the differential becomes the first v1 add.
- **One-shot corpus** — a single manual Snowflake pull of the **5 mature
  dimensions** (`sender`, `move_call`, `emit_module`, `event_type`,
  `affected_object`) at a few selectivity tiers (§4.2), **plus one hand-built
  degenerate entry** for question 2. **One dual-purpose library** (correctness
  oracle first, then load fodder), **split shared (recent, both backends) vs
  archival-only (deep / full history)** — schema + design in §4.6. Saved as JSONL
  next to the harness. **Not** versioned/automated/monthly-regenerated yet —
  that's v1.

### Reproducibility-lite (so a re-run means something)

Even manually, make a re-run hit the same data and same requests, cheaply:

- **Freeze a checkpoint ceiling.** Pin `CP_CEILING` and set `before = CP_CEILING`
  on every query. History below a final checkpoint is immutable, so the ceiling
  turns the still-growing store into a fixed dataset **with zero snapshot infra** —
  same inputs over the same data → comparable numbers, even by hand. (kv-rpc
  freezes this way trivially; the fullnode can't freeze deep history — another
  reason its v0 role is the recent-window differential, not frozen perf.)
- **Save the manifest beside the results:** `{corpus file, cp_ceiling, seed,
  mix/profile, code rev}`. Git is the input database. No runtime input-logging DB
  needed — generate the request list ahead of time from the seed so the run is
  deterministic and the manifest fully reproduces it.

### Explicitly deferred out of v0 (do NOT build yet)

k6-operator distribution & cross-pod aggregation · any CI wiring
(`workflow_dispatch`/nightly/weekly) · perf regression trend store · monthly
corpus regeneration · the Snowflake warehouse oracle as a tiling check ·
`event_stream_head` (not launched) · a dedicated BigTable-over-provisioned load
target + archival fullnode · generated-TS-types polish (raw requests are fine for
v0). These are the §2–§6 vision; revisit once v0 has cleared the announcement.

## 2. Performance testing (primary)

### 2.1 Load tooling — DECISION: use k6 (Grafana), don't build our own

**k6** is Grafana Labs' open-source load tool (engine in Go, scripts in JS/TS).
A k6-based harness already exists in the **`sui-operations`** repo, set up by
**rush** (`rushrs`) in Aug–Sep 2025 (`k6-operator` on GKE, deployed via Pulumi;
scripts under `pulumi/services/k6-operator/k6-load-tests/scripts/`; manual
`workflow_dispatch` via `.github/workflows/k6-quick-test.yml`; metrics →
`grafana.sui.io`). It is **JSON-RPC + GraphQL only — no gRPC** — and has been in
pure maintenance mode since rush built it.

**Why adopt k6 rather than build a Rust tool:** its value is the parts that are
expensive and easy-to-get-wrong to rebuild, not the request loop:

1. **Distributed, k8s-native scale-out** (k6-operator) + **correct cross-pod
   result aggregation** (you cannot average per-pod percentiles).
2. **Open-loop arrival-rate executors** (`ramping-arrival-rate`) — coordinated-
   omission-free, native.
3. **Grafana metrics pipeline** already wired (this is also the only existing
   "home" for perf time-series; there is no in-repo regression-tracking
   convention to inherit).

k6 is **protocol-agnostic infra**, so — unlike `sui-rpc-loadgen` — it does **not**
die with JSON-RPC (which is being decommissioned ~July 2026). We considered and
rejected reusing `sui-rpc-loadgen` (JSON-RPC/SDK-bound, dormant, unscheduled).

**The friction with k6 is gRPC server-streaming + JS.** Mitigations, in order:

- **Generated TS types** from the protos (`buf generate` / ts-proto) give
  compile-checked request construction (the gnarly DNF filter objects) — you do
  *not* get a generated client (k6's Go engine does the wire work via dynamic
  `invoke`/`Stream`); responses are an unchecked cast. Still removes ~80% of the
  untyped-JS pain.
- **k6 `grpc.Stream`** is event/callback-based (`on('data')` per frame,
  `on('end')`), **not** an all-at-once vec dump — so per-frame latency
  (time-to-first-frame, drain time) is measurable, but the instrumentation is
  hand-rolled in JS via custom `Trend`/`Counter` metrics. `client.invoke()` is
  unary-only — must use `Stream` for List*.
- **xk6 extension (Go)** is the escape hatch if stock-k6 streaming is too rough:
  write the streaming/metrics/pagination ergonomically in Go with typed Go stubs,
  expose a thin JS surface. Cost: a Go project to maintain + a custom k6 image the
  operator must run. **Deferred behind the streaming spike (§2.5).**

> Observation: every k6 mitigation moves the hard logic into a typed/compiled
> layer. The muddy middle (untyped JS + hand-rolled streaming) is the thing to
> avoid. If the spike shows streaming is a real fight, the coherent fallback is a
> standalone Rust harness (tonic + `scan-history-bench`'s proven inner loop,
> shared with the correctness oracle) — accepting that you'd rebuild
> distribution/metrics.

### 2.2 Ramp model

- **Open-loop arrival-rate**, never closed-loop (closed-loop can't find a knee:
  built-in backpressure + coordinated omission).
- Step up offered RPS by `M` every `N` time units until the knee.
- **The knee is where goodput stops tracking offered load** — NOT a latency
  percentile. The RPC service load-sheds (`grpc_load_shed` / concurrency limit),
  which *fast-fails* and *improves* p99-on-success right as it collapses. So:
  - track **goodput (successful streams/sec) vs offered RPS**,
  - measure latency **including** errors/timeouts and from **intended-send-time**,
  - **kill the run on error-rate threshold** (errors hide the latency knee).
- **Per-shape ramps** (clean knee per cost class) **and** a realistic-mix ramp
  (the operational SLO number). Cost varies by orders of magnitude across shapes.
- **Warm up then measure** at each step (caches, connections, package-resolver
  LRU). Cold-vs-warm is itself a dimension.

### 2.3 Query shapes (the cost spectrum)

- **cheap:** digest-only `read_mask`, unfiltered, narrow range.
- **expensive:** heavy `read_mask` (bodies + objects + JSON/package resolution),
  wide range.
- **adversarial:** ultra-sparse / degenerate filter that burns the whole scan
  budget returning ~0 (the `system-sender ∧ pyth`-style empty intersection). The
  per-request budget *bounds* each one; the realistic DoS vector is a **flood** of
  them — characterize that explicitly.
- **mix:** weighted by expected production traffic, and **including the v2
  point-reads** (GetCheckpoint/Transaction/Object, ListOwnedObjects, GetBalance)
  that share the same RPC process and contend for resources. Testing List* in
  isolation overstates capacity.

### 2.4 Target infra & the bottleneck experiment

- **Saturate one RPC replica** → per-replica knee = the capacity-planning unit
  (replicas ≈ peak_RPS ÷ per-replica-knee × headroom).
- **Isolate a single pod (in-cluster) — do NOT load through `port-forward`.** The
  §2.5/§2.6 `kubectl port-forward` was laptop-side spike convenience only; it
  tunnels one TCP stream through the kube-apiserver and caps throughput, so you'd
  measure its knee, not the pod's. Run the generator *in-cluster* and target one
  pod directly (pod IP, or a pinned single-pod Service / StatefulSet per-pod DNS)
  to bypass the Service's L4 per-connection balancing. **Disable HPA/KEDA** for
  the window (autoscale hides the per-pod knee and fakes "graceful" degradation),
  pin the pod's CPU/mem limits, and put the generator on a different node.
- **BigTable must not be the bottleneck — prove it, don't assume it.** Experiment:
  the knee must **rise when you scale RPC replicas** and **stay put when you scale
  BigTable nodes**. Watch BigTable CPU/node during ramps. The RPC service's *own*
  limits (`request_bigtable_concurrency`, scan budgets, caches) are **in scope** —
  it's BigTable *throughput* we provision out of the picture.
- Existing asset: a mainnet-backfilled kv-rpc testbed exists (see memory
  `kv-rpc-bitmap-scan-testbed`). A matching archival fullnode is the harder piece;
  Pulumi for the target env is a parallel track (TBD — Nick to point at it).

### 2.5 Streaming spike — RESOLVED ✅ (2026-06-24): stock k6 works

**Question:** can k6 `grpc.Stream` (a) open a `ListTransactions` server-stream,
(b) drive it under `ramping-arrival-rate`, and (c) emit clean per-call latency
Trends (TTFF, drain) + a goodput counter — without reintroducing coordinated
omission? **Answer: yes, stock k6.** Spike ran against the mainnet fullnode
(`sui-node-mainnet-rpc-alpha-http2` via port-forward). Verdict + proof script in
the spike scratch dir (`liststream.k6.js`, `VERDICT.md`).

**What was proven:**
- The iteration-held-open pattern works: wrap the stream in a `Promise` that
  resolves only on `on('end')`/`on('error')`, `await` it in an `async` default fn.
  TTFF 54ms / drain 105ms on a 95-item stream (not ~0 → no fire-and-forget).
- **Iterations genuinely held open**, confirmed by Little's law: peak concurrency
  `max 6 VUs ≈ offered_rate(80/s) × drain(0.07s)`. Fire-and-forget would show ~1.
- **k6's open-loop accounting is honest:** a VU-starved A/B (`constant-arrival-rate`
  100/s, `maxVUs:3`) reported `delivered 459 + dropped_iterations 742 ≈ 1200
  offered` — a closed-loop tool would have silently throttled to 38/s.

**What was NOT proven (still open — don't over-read the ✅):**
- **Coordinated-omission was demonstrated under *VU* starvation, not *server*
  saturation.** The gentle ramp never reached the server knee (0 errors, 6
  concurrent). Real load-shed behaviour (latency-incl-errors from intended-send-
  time when the *server* sheds) is a different regime — validate it during the
  capacity test (§2.4), on a target we're allowed to saturate.
- **Single-page streams only.** The spike drove one stream to natural completion;
  **cursor pagination across `SCAN_LIMIT` was not exercised.** That is the same
  thing as the iteration-granularity question (§7) and is now the top unknown —
  see §2.6.
- **Fullnode/RocksDB backend only** (not kv-rpc/BigTable), and **one stream per
  channel** (no HTTP/2 multiplexing of concurrent streams over one channel). Both
  are capacity-phase concerns; wire mechanics are identical so the tooling
  decision transfers.

**Setup gotchas (carry into the real harness):**
- **TLS, not plaintext.** `9443` behind the `-http2` service is h2-over-TLS.
  `grpcurl -plaintext` times out, `-insecure` works. k6: connect `{ plaintext:
  false }` + top-level option `insecureSkipTLSVerify: true` (localhost cert
  mismatch behind the port-forward).
- **Two proto roots in `client.load`.** `ledger_service.proto` is under
  `sui-rpc/proto`, but its `sui/rpc/v2/*` + `google/protobuf/*` imports are under
  `sui-rpc/vendored/proto`. Pass **both** or you get `no such file`.
- **FieldMask encoding is tool-specific.** k6's protojson wants the canonical
  string `read_mask: 'transaction.digest'`; grpcurl wants `{ paths: [...] }`.
- **Addresses must be 32-byte padded** (`0x00…03`, not `0x3`) in `move_call`
  (and likely sender/object) filters, or the server rejects `invalid address`.
- **Responses are untyped** (`msg.item` / `msg.watermark` / `msg.end` plain JS
  objects). Fine for v0; generated TS types (§2.1) remove most of this later.
- **Custom Trends only surface in the summary if referenced** — populate them, or
  add an always-true `thresholds` entry (`['p(95)>=0']`) to force them into the
  end-of-run table.

**Decision confirmed:** stock k6 for v0. Defer xk6-Go to *if/when* pagination-
across-`SCAN_LIMIT` ergonomics bite (a Go helper for *pagination*, not basic
streaming). Standalone Rust fallback not needed.

### 2.6 Pagination / iteration granularity — RESOLVED ✅ (2026-06-24)

The streaming spike (§2.5) proved the single-stream primitive; this one proved
the **multi-page composition** and settled the **iteration-granularity** modeling
call (§7). Ran against the controllable kv-rpc testbed (BigTable, full mainnet
history, tip 288.9M; **plaintext h2c on :8000**, unlike the TLS fullnode).
Verdict + proof script in the spike scratch dir (`paginate.k6.js`,
`VERDICT_PAGINATION.md`).

**Q1 — can stock k6 thread the cursor across a chain in plain JS? → YES,
trivially.** A ~15-line `while` loop carries `lastCursor` and re-issues each page
with `options.after`. A full-history degenerate drain threaded **42 pages
genesis→tip** (41 `SCAN_LIMIT` + 1 `LEDGER_TIP`), `lastHi` == exact ledger tip,
**0 watermark-monotonicity violations**, 0 errors. The `bytes` cursor round-trips
as a base64 string through `options.after` with no special handling. **xk6-Go is
NOT reconsidered** — the ergonomics that were its trigger did not bite.

**Q2 — one-page-per-iteration vs full-query-drain-per-iteration? → ONE PAGE for
the LOAD harness; full-drain for CORRECTNESS only.**
- **Model A (one page / iteration, arrival-rate = pages/sec)** is a *linear,
  controllable* load knob: uniform ~110ms iterations, concurrency ≈ 1 (Little's
  law), per-page latency = the honest unit of server work. It cleanly found the
  **per-pod knee for the degenerate worst-case scan ≈ 4–6 pages/sec** (healthy
  ≤4/s @110ms; sharp open-loop collapse ≥8/s — CPU-bound bucket walks spiral).
- **Model B (full drain / iteration, arrival-rate = queries/sec)** is honest but
  **unusable as a load knob**: one in-flight drain already emits pages
  sequentially at ~5/sec (≈ one pod's knee), and induced page-rate =
  `drains/sec × pages_per_drain` where `pages_per_drain` is history-depth-
  dependent and unbounded (42 for the degenerate filter, *thousands* for a dense
  drain). Overlapping drains spiral into `DEADLINE_EXCEEDED`, and errored pages
  break the resume loop so `pages_per_query` decays from a clean 42 to a ragged
  1–42 (tiling destroyed). Keep Model B *low-concurrency* as the **correctness
  oracle** (∪ pages == one query, watermark monotonicity, non-overlap — it nails
  42/42 every time when unsaturated).

**Corpus refinement:** Model-A iterations can be seeded by a **checkpoint window**
(`start_checkpoint`/`end_checkpoint`, which the Snowflake corpus §4 already
produces) instead of an opaque cursor. Cursors are opaque position-encoded
`bytes` you can't synthesize from a checkpoint, so seeding by checkpoint sidesteps
a cursor-sampling corpus; **opaque cursors are only needed for Model B's
intra-query resume.**

**Decision (§7 iteration granularity):** arrival-rate = **pages/sec, one page per
iteration, seed by checkpoint window.**

**Gotchas (carry into the harness):**
- Under overload, requests fail with **gRPC code 4 `DEADLINE_EXCEEDED`**
  (`"list_transactions request deadline exceeded"`) at a **~5s per-request
  deadline** — i.e. doomed requests run to the deadline rather than fast-rejecting.
  k6 records the full 5.03s intended-send-time latency + the error. Observed
  incidentally here; whether this degrades cleanly (esp. whether a degenerate flood
  starves concurrent point-reads) is left to the v0 abuse-resistance test
  (question 2) to characterize.
- **k6 fires BOTH `on('error')` and `on('end')` for a failed stream** → every
  errored page double-counts (`stream_errors == spurious end_terminal`); guard
  with a per-stream `settled` flag (first callback wins).
- Track the latest cursor from **both** standalone `watermark` frames (SCAN_LIMIT,
  0 items) and `item.watermark` (ITEM_LIMIT), or a degenerate drain never advances.
- `SenderFilter` is `{ address }`, not a bare string (every predicate is a wrapper
  message: `move_call{function}`, `affected_object{object_id}`, …).

## 3. Correctness testing (secondary — already partly covered)

**Existing build-time coverage (keep as-is, runs in `rust.yml`):**

- `crates/sui-indexer-alt-e2e-tests/tests/kv_rpc_tests.rs` (~4000 lines) —
  List* filters: sender, move_call, emit_module, event_type, affected_address,
  affected_object, event_stream_head, package_write; AND/OR/NOT DNF, unanchored
  negation, generics, cursor monotonicity, pagination non-overlap.
- `crates/sui-kvstore/tests/bitmap_query.rs` — bitmap AND/OR/NOT vs hand-built
  expected sets.
- Built on **`FullCluster` + Simulacrum** as deterministic ground truth, with a
  watermark-sync wait pattern.

**Gaps (require the new harness, not a unit test):**

1. Real-data scale & diversity (Simulacrum is tiny/synthetic).
2. **Cross-backend differential** — kv-rpc vs fullnode for the same query. Does
   **not** exist anywhere today; cheapest high-scale regression catcher.
3. Over-the-wire correctness on real historical ranges.

**Layered oracle strategy (by data regime):**

- **Brute-force oracle** → synthetic/adversarial (re-derive matches from
  unfiltered contents; depends on nothing else being correct). Build-time.
- **Cross-backend differential** → recent rolling window (both backends trivially
  have it; catches code regressions). The Rust/oracle half of the harness.
- **Warehouse oracle (Snowflake)** → deep historical breadth without a second
  archival fullnode. Spot-check, not tile (SQL must mirror DNF exactly).
- **Algebraic / metamorphic decomposition** → OR/NOT on **real data**, no faithful
  DNF SQL: `A∨B==A∪B`, `A∧¬B==A∖(A∧B)`, `¬B==unfiltered(R)∖B`, checked by diffing
  RPC result-sets (order-independent) + the count identity `|A∪B|=|A|+|B|−|A∧B|`
  for ranges too big to materialize. Sound only because the single-literal
  primitives stay externally anchored (exact Snowflake count + differential).

Invariants to assert explicitly: pagination tiling (∪ pages == one query),
watermark honesty (`checkpoint_hi/lo` never over-claims), asc == reverse(desc),
light/heavy `read_mask` agreement, scan-limit resume (no gaps/dupes), and the
combinator identities `A∨B==A∪B` / `A∧¬B==A∖(A∧B)` / `¬B==unfiltered∖B`.

## 4. Workload generation (the hard part — perf corpus)

The corpus quality determines whether the load test means anything. Random filter
values are useless (a random sender sent nothing → empty-path, no index work).

**Source: Snowflake** `ANALYTICS_DB_V2`, schemas `CHAINDATA_MAINNET` /
`CHAINDATA_TESTNET` (see memory `snowflake-cli-chaindata`). CLI:
`snow sql -c nick --warehouse ANALYTICS_WH -q "..."`.

### 4.1 Dimension → column mapping (verified)

Every dimension table carries a **`CHECKPOINT`** column — the join key to the
APIs' checkpoint-range filtering, and what lets us classify *where in history* a
value is dense.

| gRPC filter | Snowflake source |
|---|---|
| `sender` | `TRANSACTION.SENDER` (also `EVENT.SENDER`) |
| `move_call` | `MOVE_CALL.PACKAGE / MODULE / FUNCTION_` |
| `emit_module` | `EVENT.PACKAGE / MODULE` |
| `event_type` | `EVENT.EVENT_TYPE` |
| `affected_object` | `TRANSACTION_OBJECT` (changed statuses — see §4.3) |
| `affected_address` | `OBJECT.OWNER_ADDRESS` + accumulator balance (see §4.3) |
| `package_write` | `MOVE_PACKAGE` |
| `event_stream_head` | ⚠️ scoped out — see §4.4 |

### 4.2 Selectivity tiering

For any candidate value, one `GROUP BY` yields `(count, MIN/MAX(CHECKPOINT),
cp-bucket histogram)` = selectivity + historical density → tiers:

- **dense-everywhere** (high count, span ≈ genesis→tip, uniform) — system sender,
  framework move-call.
- **recent-only** (`MIN(CHECKPOINT)` late) — a current package id.
- **sparse** (low count, wide span).
- **clustered/bursty** (concentrated in a few cp buckets).
- **empty-degenerate** — manufactured & **verified in Snowflake** by confirming
  `COUNT(*)` of a two-dense-dim conjunction is ~0 over a range.

**Bonus:** the expected result count per `(filter, cp-range)` doubles as a free
correctness signal and labels each corpus entry with its true cost class.

### 4.3 `affected_*` is losslessly reconstructable in SQL (verified)

(Corrected from an earlier "approximate" hand-wave — it's exactly defined in
`crates/sui-inverted-index/src/dimensions.rs` and every input is in the
warehouse; it's a join, not a single column.)

- **`affected_object`** = object ids of `effects.object_changes()` =
  `TRANSACTION_OBJECT` filtered to **changed** statuses (`Mutated/Created/Deleted/
  Wrapped/Unwrapped`), **excluding** the large `OBJECT_STATUS='None'` read-input
  rows.
- **`affected_address`** = (1) `OBJECT.OWNER_ADDRESS where OWNER_TYPE='AddressOwner'`
  at **both** input- and output-versions of each changed object, **∪** (2)
  balance-accumulator owners. Verified edge: the warehouse folds
  `ConsensusAddressOwner` into `OWNER_TYPE='AddressOwner'` (flagged by
  `IS_CONSENSUS`), with `OWNER_ADDRESS` populated — so the plain owner filter
  matches the Rust `owner_as_affected_address` predicate exactly (Shared /
  ObjectOwner / Immutable / None correctly excluded). The balance half is
  recent-only (~cp 288M+) and needs a JSON-key dig from the accumulator
  `Field<accumulator::Key<Balance<T>>, U128>` rows.

**Crucial distinction:** losslessness matters for the **correctness oracle**
(exact result sets/counts), **not** for the **load corpus** — for the corpus, any
valid owner address sampled from `OBJECT` is a fine filter value with real
selectivity. Don't over-engineer the corpus extraction.

### 4.4 Verified availability findings

- Accumulator framework went live on **mainnet only ~cp 288.18M** (weeks ago) and
  currently powers **balances only**.
- **`event_stream_head` (`0x2::accumulator_settlement::EventStreamHead`, a dynamic
  field under `0x2::accumulator::AccumulatorRoot`) is NOT on mainnet** (zero rows)
  — new, unlaunched feature; only ~3 streams ever on testnet.
  - **DECISION:** non-goal for the mainnet load corpus. Represent only as a
    synthetic valid-but-empty / scan-budget-burner entry. Correctness already
    covered by the `authenticated_event` e2e/Simulacrum tests. Revisit at launch.
  - Note: the filter's `stream_id` is the accumulator `Key` address (in
    `OBJECT_JSON`), **not** the dynamic field's `OBJECT_ID`.
- `affected_address`'s balance half is recent-only → weight it recent-range, not
  full-history.

### 4.5 Corpus discipline — DECISION: versioned static, regenerated deliberately

- **Checked-in, versioned JSONL corpus** for the regression-tracked ramps (hold
  workload constant, vary only code → detect regressions). Per-run randomness is
  **harmful** for regression detection.
- **Regenerate from Snowflake on a cadence (monthly-ish) as a version bump** so it
  doesn't rot and you don't overfit the server to a frozen corpus.
- A separate **dynamic/exploratory tier** is fine for *discovery* of unanticipated
  breaking shapes — kept out of the regression baseline.
- Extraction is a periodic batch job (tables are billions of rows: `MOVE_CALL`
  12.8B, `EVENT` 7.6B, `OBJECT` 30B, `TRANSACTION` 5.4B), sampled — never per-test.

### 4.6 Test-case set design (v0 corpus — the dual-purpose library)

The v0 corpus is **one test-case library, two consumption modes**: build it and
verify **correctness first**, then replay the *same* records as load to find the
breaking point (questions 1 & 2). Each record is self-describing so one file
feeds both.

**Backend partition (fullnode and archival share one wire interface, §1):**
- **Shared set** — recent window `[CP_CEILING − W, CP_CEILING]`, runnable on
  *both* backends. Every dimension/combinator/specificity family lives here; this
  is what powers the cross-backend differential (§3, question 3).
- **Archival-only set** — the *same shapes* over deep / full-history ranges
  (genesis→tip) the fullnode can't serve (pruned). Oracle is the warehouse (§3),
  since there is no second backend that deep. Owns full-history drains, the
  genesis/tip edges, and the dense-everywhere cost monsters.

**Representation — `corpus.jsonl` (one case/line) + sidecar `manifest.json` +
`queries/*.sql`.** Each record = a metadata envelope around a **verbatim
protojson `request`** that maps 1:1 to
`List{Transactions,Events,Checkpoints}Request` — exactly what k6 sends (no
translation), and what the Rust correctness side parses into the same proto type
(one source of truth, no bespoke filter parser, no load-vs-correctness drift):
```
{ "id": "tx.sender.na.dense_everywhere.shared.expensive.<vh>",
  "rpc": "ListTransactions",               // ListTransactions | ListEvents | ListCheckpoints
  "request": {                             // verbatim send payload (snake_case protojson, proto rev 43c5bc1)
    "start_checkpoint": 278000000,         // inclusive
    "end_checkpoint": 288000000,           // EXCLUSIVE (== the frozen CP_CEILING)
    "filter": {"terms":[{"literals":[{"include":{"sender":{"address":"0x00..0a"}}}]}]},
    "read_mask": "transaction.digest",     // FieldMask comma-joined path string; omit -> server default
    "options": {"limit_items": 1000}       // ordering (ORDERING_DESCENDING) + after/before live here too
  },
  "class": {"dimension":"sender", "combinator":"single", "selectivity_tier":"dense_everywhere",
            "cost_class":"expensive", "backend_scope":"shared", "specificity":"na"},
  "oracle": {"kind":"exact_count", "expected_count":123456, "sql_ref":"extract.py:_match(sender,na)"} }
```
- **`request`** maps 1:1 onto `List{Transactions,Events,Checkpoints}Request`:
  `read_mask`, `start_checkpoint` (incl), `end_checkpoint` (**excl**), `filter`
  (`TransactionFilter` for txns/checkpoints, `EventFilter` for events),
  `options{limit_items, ordering, after, before}`. DNF = `terms[]` (OR) of
  `literals[]` (AND) of `{include|exclude:<predicate>}`; every predicate is a
  wrapper message; addresses MUST be 32-byte-padded; `filter` omitted = unfiltered.
  **snake_case** protojson — the casing proven with k6 in the spikes (§2.6); the
  builder (`corpus_builder.py`) emits it so nobody hand-writes the nested wrappers.
- **`oracle.kind`** ∈ `exact_count | decomposition | membership | degenerate`. For
  combinators (§3), `kind:"decomposition"` with `relation:"union"|"difference"`
  and `components:[<ids>]` name the sub-cases whose results compose the
  expectation — the runner derives the oracle from cases it already runs.
- **`manifest.json`** (sidecar, one object): `{cp_ceiling, window_W, corpus_rev,
  snowflake_account/warehouse, seed, generated_at}`. Git is the input DB (v0
  reproducibility-lite).

**RPC × dimension applicability (generation MUST respect — event-space is
narrower than tx-space):**

| dimension | ListTransactions | ListEvents | ListCheckpoints |
|---|---|---|---|
| `sender` | ✓ | ✓ | ✓ |
| `emit_module` / `event_type` / `event_stream_head` | ✓ | ✓ | ✓ |
| `affected_address` / `affected_object` / `move_call` / `package_write` | ✓ | ✗ | ✓ |
| (unfiltered) | ✓ | ✓ | ✓ |

`EventPredicate` only carries sender/emit_module/event_type/event_stream_head
(verified). ListCheckpoints = "checkpoints containing a matching tx" → takes the
full tx-space predicate set.

**Oracle strategy is per-shape.** Complex DNF in SQL is hard for **semantic
fidelity**, not data volume (`COUNT(*)` is cheap at any scale): the SQL must
reproduce the index's DNF exactly — `COUNT(DISTINCT)` at the RPC's output grain
(tx / event / checkpoint), `NOT` as a `NOT EXISTS` anti-join (a `!=` join
double-counts and mis-excludes), and the `affected_*` reconstruction (§4.3) under
every term. So:
- **single literal & simple AND** → exact Snowflake `COUNT(*)` full-range (the
  §4.2 group-by already produces it; doubles as `expected_count` + cost label).
- **OR / NOT / unanchored-negation** → **algebraic decomposition** is the primary
  real-data oracle, no faithful DNF SQL: `A∨B==A∪B`, `A∧¬B==A∖(A∧B)`,
  `¬B==unfiltered(R)∖B` — diff RPC result-sets, with `|A∪B|=|A|+|B|−|A∧B|` for
  ranges too big to materialize. Sound because the single-literal/AND primitives
  stay externally anchored (exact `COUNT(*)` + differential). Backstops: the
  **cross-backend differential** on the OR/NOT query itself (shared set), and
  **windowed-exact** SQL (`UNION` / `NOT EXISTS`, narrow window) for an external
  non-RPC check. Build-time e2e already exact-oracles these on synthetic data —
  this closes the real-data gap.
- **deep DNF / `affected_*` under combinators** → windowed-exact + point-membership
  sampling (N returned + N random in-range, RPC ⟺ SQL both directions); the only
  oracle on **archival-only deep** ranges (no differential there).
- **degenerate / adversarial** → oracle is "≈0 items, `end_reason = SCAN_LIMIT`,
  replica stays up" (question 2), not a count.

**Specificity axis (compound dims = a first-class cost lever):** `move_call` at
`pkg` / `pkg::module` / `pkg::module::fn`; `emit_module` at `pkg` / `pkg::module`;
`event_type` at `addr` / `::module` / `::Name` / `::Name<type_params>` (generic
instantiation). Coarser prefix = denser = more scan work; sample each level.

**Sampling discipline — a spine, then deepen only where the 3 questions need it.**
One representative per `(dimension × combinator × specificity)` family is the
correctness spine (broad, shallow, small item limits); then deepen on
**cost_class / read_mask / tier** for capacity (Q1) and on the **degenerate
family** for abuse (Q2). NOT the full cross product.

**Invariants asserted per case (correctness pass, §3):** tiling (∪ pages == one
query), watermark honesty (`checkpoint_hi/lo` never over-claims), asc ==
reverse(desc), light/heavy `read_mask` agreement, scan-limit resume (no
gaps/dupes), `expected_count` match where an exact oracle exists.

## 5. CI integration

- **Never block merges.** Heavy tests are scheduled/manual, like everything
  expensive in this repo.
- **Template: `simulator-nightly.yml`** — GitHub Actions orchestrates; the heavy
  run executes on dedicated infra (there: Teleport SSH to `simtest-01` + detached
  `nohup` + poll a status file). For us: swap in a kubectl/Pulumi job on GKE,
  poll, done. Notify `MystenLabs/sui-operations` via `repository-dispatch` on
  failure (it owns the Slack notification); non-blocking.
- **Cadence:**
  - **`workflow_dispatch` (params: ref, target, shape, duration)** — build first;
    the manual-run path.
  - **Nightly cron** — cross-backend differential on a recent rolling window.
  - **Weekly cron** — full load sweep + warehouse historical correctness.
- **Open gap:** no in-repo convention for where perf numbers land for regression
  tracking (Slack + Grafana + StepSummary exist; no time-series baseline store).
  Grafana (via k6) is the natural home — decide and standardize.

### Prior art / cross-team context

The alt-indexing stack's CI lives in **`sui-operations`** (not the monorepo): a
continuously-deployed full alt stack in the **`ci` env** (GKE
`workloads-secondary-use4`, ns `rpc-ci`), one `Pulumi.ci.yaml` per service.
`continuous.yml` (every 3h) build→deploy→validate; the validation
(`devnet-rpc-deploy.yml`) is **smoke + freshness/lag** (Grafana queries) + one
quasi-correctness check (fullnode-vs-graphql checkpoint gap narrowing =
eventual-consistency, **not** data correctness). **There is no data-payload
correctness oracle anywhere, and no gRPC coverage.**

**Cross-team framing:** Nick is on a sister team to the alt-stack owners. The plan
is to get gRPC tested, then raise the bar *with* that team: **we bring the
correctness oracle nobody has; we ride their k6/Grafana/Pulumi rails for load.**
Adding gRPC to the org's shared k6 framework *is itself* raising the bar.

## 6. Decisions log

1. **Perf testing first**, correctness second (correctness is well-covered at
   unit/e2e level; perf is the real gap).
2. **Use k6** for load (Grafana's tool + rush's existing framework), **don't build
   a bespoke load engine**. Add a gRPC v2alpha workload + profile.
3. **Don't reuse `sui-rpc-loadgen`** (JSON-RPC-bound, dies with JSON-RPC ~July 2026).
4. **Stock k6 confirmed (streaming spike, 2026-06-24)** — server-streaming under
   open-loop arrival-rate with honest, coordinated-omission-free latency works in
   plain JS (§2.5), and **cursor pagination threads ergonomically in ~15 lines of
   JS** (§2.6) — **xk6-Go is no longer on the table**; the trigger that would have
   reconsidered it (pagination ergonomics) was tested and did not bite.
   **Generated TS types** remain a later ergonomic add; **standalone Rust**
   fallback not needed.
4b. **Iteration granularity (§2.6, §7): one page = one iteration, arrival-rate =
   pages/sec, seed by checkpoint window.** Full-query-drain iterations are
   correctness-only (tiling / watermark monotonicity), never the load model.
5. **Open-loop arrival-rate ramp**; knee = goodput diverging from offered load;
   error-rate kill; per-shape + mixed ramps.
6. **Saturate one RPC replica**; prove RPC (not BigTable) is the bottleneck via the
   scale-replicas-vs-scale-bigtable experiment.
7. **Fullnode & archival share one harness**, different expectations.
8. **Corpus from Snowflake**, selectivity-tiered via `CHECKPOINT`; **versioned
   static, regenerated monthly**; not per-run random.
9. **`event_stream_head` scoped out** of the mainnet load corpus (not launched);
   `affected_address` balance half weighted recent-only.
10. **`affected_*` are losslessly SQL-reconstructable** (for the oracle); the load
    corpus doesn't need losslessness.
11. **CI non-blocking**, `simulator-nightly.yml` pattern, manual→nightly→weekly.
12. **v0 = a manually-run test, no CI** (see v0 milestone section). Scope is exactly
    the three questions — rough per-replica capacity, graceful degradation under a
    degenerate-query flood, and a cross-backend correctness sanity-check on real
    recent data. Single load generator, existing kv-rpc testbed, one-shot corpus,
    ceiling-frozen for reproducibility. Distribution / CI / regression-tracking /
    warehouse-tiling are all v1+.
13. **v0 corpus is one dual-purpose library, corpus-first** (§4.6): build via snow
    CLI → verify correctness (cross-backend differential on a **shared** recent
    set; warehouse spot-check on the **archival-only** deep-history set) → replay
    the *same* records as load to the breaking point. Oracle is per-shape (exact
    Snowflake count for single-literal/simple-AND; differential for complex DNF).
    Single pod targeted in-cluster, not via `port-forward`. Manual 1-off, matured
    with the team over the coming weeks.

## 7. Open questions / next steps

**v0-blocking (do these to ship v0):**

- ~~The streaming spike (§2.5)~~ — **RESOLVED ✅ (2026-06-24): stock k6.** See §2.5.
- ~~Pagination / iteration-granularity spike (§2.6)~~ — **RESOLVED ✅
  (2026-06-24): stock k6, one-page-per-iteration (pages/sec), seed by checkpoint
  window; full-drain is correctness-only; xk6-Go still not needed.** See §2.6.
- **Does a recent-window fullnode exist (or is it cheap to stand up) for the
  differential?** If yes, the cross-backend differential is in v0; if no, v0
  correctness leans on the existing e2e tests + a warehouse spot-check and the
  differential slips to the first v1 add.
- **One-shot corpus pull** — build the dual-purpose library per §4.6 (5 mature
  dimensions at a few tiers + one degenerate entry; shared vs archival-only
  split); saved JSONL. (Value sourcing = §4.2 group-by; extraction SQL below.)

**v1+ (deferred — documented for direction):**

- **Iteration granularity** — *merged into the §2.6 pagination spike* (it's the same
  question: one-page-per-iteration vs full-drain decides arrival-rate semantics and
  whether the corpus stores cursors). Listed under v0-blocking above.
- **Corpus tier weights**: match production frequency vs. deliberately over-weight
  expensive/adversarial shapes? Where does the API-mix ratio come from (prod
  telemetry vs documented assumption)?
- **Target infra / Pulumi** for a dedicated, BigTable-over-provisioned kv-rpc (+
  archival fullnode) load target — Nick to provide the Pulumi entry point.
- **Perf metrics home** for regression tracking (standardize on Grafana?).
- **Extraction SQL** for the 5 mature dimensions + the lossless `affected_*`
  oracle query.

## Appendix: key references

- APIs: `crates/sui-kv-rpc/src/v2alpha/list_{checkpoints,transactions,events}.rs`,
  `crates/sui-rpc-api/src/grpc/v2alpha/ledger_service/list_*.rs`
- Contract: `crates/sui-rpc-api/src/ledger_history/{query_options,watermark,filter}.rs`
- Bitmap index: `crates/sui-inverted-index/src/{dimensions.rs,bitmap_query/}`,
  `crates/sui-kvstore/src/handlers/bitmap/`
- Existing tests: `crates/sui-indexer-alt-e2e-tests/tests/kv_rpc_tests.rs`,
  `crates/sui-kvstore/tests/bitmap_query.rs`
- k6 framework: `sui-operations` repo,
  `pulumi/services/k6-operator/k6-load-tests/`, `.github/workflows/k6-quick-test.yml`
- CI template: `.github/workflows/simulator-nightly.yml`
- Throwaway gRPC probe (proves the inner loop): `scan-history-bench` on branch
  `nickv/scan-history-bench`
