<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Hand-off: v2alpha gRPC List API perf investigation

This is a fresh-start briefing. It gives you the **problem, the map (code/infra/
tooling), how to run things, and what state the infra is currently in.** Prior
findings are recorded separately and explicitly marked **re-verify** — treat them
as leads that may be wrong, not as established truth. Nothing here should anchor
your diagnosis.

---

## 1. Objective

> Figure out why we can't hit an actual physical saturation limit (network IO,
> CPU, etc.) on a single `sui-kv-rpc` (archival gRPC) replica, and why request
> latency knees hard under load before that.

Context: we're load/perf + correctness testing the new
`sui.rpc.v2alpha.LedgerService` streaming List APIs (`ListCheckpoints`,
`ListTransactions`, `ListEvents`) before announcing them. The broader design doc
is `testing_plan.md` in this worktree — read it for the full picture, selectivity
tiers, query shapes, and the correctness plan; this hand-off is scoped to the
"can't saturate / premature latency knee" question.

---

## 2. Where things live

**Code under test** (server): worktree
`~/workplace/sui/worktrees/pipelined-key-batches/`
- `crates/sui-kv-rpc/src/` — RPC service. Notable files:
  `config.rs` (all tunables), `bigtable_client.rs` (per-request BigTable limiter +
  `permit_wait_ms`/`permits_peak`/`ops_total` metrics), `pipeline.rs`
  (chunked/pipelined stream drain), `operation.rs`, `v2alpha/list_*.rs`.
- `crates/sui-kvstore/src/bigtable/client/` — BigTable client + `channel_pool.rs`
  (the connection/channel pool).
- Which SHA is deployed matters — see §4.

**Test scripts / scratch** (this worktree): `~/workplace/sui/worktrees/grpc-testing/`
- `grpc-list-testing/` — the k6 scripts: `load.k6.js` (ramping arrival-rate
  replay of a pre-generated request manifest), `fixed_shape.k6.js` (single
  worst-dense ListCheckpoints under constant-VU closed loop), `solo.probe.k6.js`
  (1-VU intrinsic-cost probe), `ping.k6.js` (preflight connectivity).
- `testing_plan.md` — design doc.
- `HANDOFF.md` — this file.

**Ops / deploy** (Pulumi): worktree
`~/workplace/sui-operations/worktrees/oneoff-perftest/`
- `pulumi/services/sui-kv-rpc/config/testnet-perftest.yaml` — the live server
  config (ConfigMap). This is where the tunables in §4 are set.
- `pulumi/services/sui-kv-rpc/` — the Pulumi stack for the `testnet-perftest`
  deployment.

**Prior-session scratch scripts** live in `/tmp` on the harness machine (NOT in
git): `conc4.sh`, `cpu_xcheck.sh`, `pool_probe.sh`, `fixed_shape.k6.js`, etc.
They encode the run patterns below but were iterated ad hoc — read before reuse,
don't trust blindly.

---

## 3. Infra / topology

Cluster (GKE): context
`gke_workloads-primary_us-east4_workloads-primary-use4`, namespace
`rpc-kv-testnet`.

- **Server under test:** Deployment `sui-kv-rpc-perftest`. Backed by a production
  testnet **BigTable** cluster with full mainnet-scale history (over-provisioned
  so BigTable itself is unlikely to be the first limit — re-confirm from GCP
  metrics if it matters). Ports: **gRPC 8000** (plaintext h2c in-cluster),
  **health 8081**, **Prometheus metrics 9184** (undeclared as a containerPort —
  reachable directly by IP/localhost, but a `Service` won't route to it).
- **Load generators:** Deployment `grpc-loadtest`, pod name prefix
  `grpc-loadtest-<hash>-<suffix>`. k6 inside each pod. Generators must run
  **in-cluster** (a laptop `kubectl port-forward` caps throughput at the apiserver
  tunnel — only used for early manual pokes, never for capacity numbers).
- One generator pod CPU-saturates its own node at high VU (each worst-dense stream
  is large), so high concurrency is reached by **spreading across several
  generator pods at moderate VU each**, not by cranking one pod.

Deploy mechanism (config-only or image change), run from the
`pulumi/services/sui-kv-rpc/` dir in the ops worktree:

```
SUI_SHA='REPLACE_WITH_GIT_SHA'
SUI_SHA_OVERRIDE="$SUI_SHA" pulumi up -s testnet-perftest --yes
```

`SUI_SHA_OVERRIDE` pins the server image to a specific `sui` commit; omit only if
you want the stack default. Editing `testnet-perftest.yaml` then `pulumi up`
rolls the ConfigMap + pod. A rollout gives a **new pod** (wipes its `/tmp`, drops
any ephemeral debug container) and can land on a **new node**.

---

## 4. Current infra mutations (state that differs from clean — revert or account for)

These were applied by the prior session for isolation/inspection. **Verify each is
still present** (a rollout may have reset some) and decide whether to keep:

- **Server pod CPU request bumped to `13500m`, memory `4Gi`, no CPU limit.**
  Verified present at hand-off time. This was a hack to force the autoscaler to
  give the pod a near-dedicated 16-core node (Burstable, can use all cores).
  Original values are in the Pulumi/backup — a prior backup was written to
  `/tmp/perftest_deploy_backup.yaml` (re-check it exists).
- **nodeSelector / tolerations: currently empty** (verified). An earlier
  experiment pinned to a `buildkit-nodes` pool via nodeSelector+toleration; that
  appears reverted. Confirm before assuming placement.
- **Ephemeral `dbg` container** (ubuntu, shares the pod PID namespace) may be
  attached for `/proc/1/stat` reads and `curl localhost:9184`. Re-check; it does
  not survive a rollout and must be re-added with `kubectl debug` if you want it.
- **Live server config knobs** in `testnet-perftest.yaml` (verified at hand-off):
  - `request-bigtable-concurrency: 100` (per-request BigTable read semaphore;
    code default is `10`).
  - `bigtable-initial/min/max-pool-size: 1000` (channel pool pinned hot to remove
    cold-start growth lag).
  - `stages: tx-seq-digest.concurrency=15`, `transactions/objects/checkpoints=50`.
  - `ledger-history.*.timeout-ms: 60000`, bitmap bucket budgets `4000`.
  - There are inline `EXPERIMENT (nickv)` comments explaining why each was set —
    read them; they encode prior reasoning (and prior assumptions).

**Re-check current live state:**

```
CTX=gke_workloads-primary_us-east4_workloads-primary-use4
NS=rpc-kv-testnet
kubectl --context "$CTX" -n "$NS" get deploy sui-kv-rpc-perftest -o yaml
kubectl --context "$CTX" -n "$NS" get pods -l app=sui-kv-rpc-perftest -o wide
kubectl --context "$CTX" -n "$NS" get deploy grpc-loadtest -o wide
```

---

## 5. How to run a load test (the mechanics)

The generic pattern the prior scripts follow:

1. **Copy the k6 script into every generator pod first** — a rollout wipes
   `/tmp`, so a run can silently no-op on pods missing the script. Preflight this
   and fail closed.
2. Launch k6 in each generator via `kubectl exec`, backgrounded, passing config
   through k6's `__ENV` (so the env var names must match what the script reads —
   e.g. `HOST`, not `H`).
3. Gate on `rpc_inflight_requests` reaching your target before you trust the
   window (streams take seconds to ramp).
4. Sample truth **from cAdvisor** (`kubectl top pod --containers`) and/or the pod
   metrics endpoint; scrape `:9184` from inside the pod (via the `dbg` container)
   or a sibling pod.
5. Clean up k6 on all gens (trap on exit) so dead clients don't leave stale
   inflight.

Env vars the k6 scripts read (confirm against the script source):
`HOST` (`host:port`), `PLAINTEXT=1`, `MAX_RECV_MB` (per-message gRPC recv cap;
worst-dense responses are large — this must be set high enough or streams error),
`VUS`, `DUR`, and for the ramping `load.k6.js`: `START_RPS`/`MAX_RPS`/`STEP_RPS`/
`STEP_DUR`, `PRE_ALLOCATED_VUS`/`MAX_VUS`, `REQ_FILE`/`FLOOR`.

Copy + launch skeleton (fill in pod suffixes; paste-safe — quote the jsonpaths):

```
CTX=gke_workloads-primary_us-east4_workloads-primary-use4
NS=rpc-kv-testnet
K=(kubectl --context "$CTX" -n "$NS")
BK=kv-rpc-http2-perftest.rpc-kv-testnet.svc.cluster.local:8000
GEN='REPLACE_WITH_GEN_POD_NAME'
POD='REPLACE_WITH_SERVER_POD_NAME'
SCRIPT=grpc-list-testing/fixed_shape.k6.js
RUNENV=(env HOST="$BK" PLAINTEXT=1 MAX_RECV_MB=128 VUS=50 DUR=260s)
"${K[@]}" cp "$SCRIPT" "$GEN":/tmp/fixed_shape.k6.js
"${K[@]}" exec "$GEN" -- "${RUNENV[@]}" k6 run --no-summary /tmp/fixed_shape.k6.js
```

Scrape metrics from inside the pod (needs the `dbg` sidecar or run from a sibling):

```
"${K[@]}" exec "$POD" -c dbg -- curl -s --max-time 8 http://localhost:9184/metrics
```

---

## 6. Metrics worth reading (names)

On `:9184`. Prior probes leaned on:
- `rpc_inflight_requests{path=...}` — concurrency actually in flight.
- `rpc_request_latency_{sum,count}{path=...}` — end-to-end per method.
- `kv_rpc_bigtable_permit_wait_ms{method,stage}` — wait to acquire a per-request
  BigTable limiter permit, by stage (`tx-seq-digest`/`transactions`/`objects`/
  `checkpoints`).
- `kv_rpc_bigtable_permits_peak{method}` — peak in-use permits per request.
- `kv_rpc_bigtable_ops_total{method}` — limiter acquisitions per request.
- `kv_bt_chunk_latency_ms{client,table}` — BigTable round-trip latency.
- `kv_rpc_stream_item_yield_wait_ms`, `kv_rpc_response_render_latency_ms`,
  `thread_stall_duration_sec` — drain/backpressure / serialization / render.
- `bt_pool_pool_size`, `bt_pool_rpcs_completed`, `bt_pool_channels_replaced` —
  channel pool.

Most are histograms (`_sum`/`_count`/`_bucket`) or counters — take **deltas over a
fixed window**, don't read absolutes. Note: cumulative counters are polluted by
prior runs since pod boot; window-delta is the only clean read.

---

## 7. Raw observations from the prior session — TIMESTAMPED ~2026-07-02, RE-VERIFY

These are leads, recorded with their evidence. **Several earlier conclusions in
the session were later overturned** (see §8), so independently reproduce anything
you intend to rely on. They are not ordered by importance and not a diagnosis.

- **CPU is not obviously idle at the knee.** On a ~5-min sustained plateau at ~200
  concurrent worst-dense ListCheckpoints, `kubectl top pod --containers` showed
  the `sui-kv-rpc-perftest` container at ~**10–12.7 cores** (node 74–94%);
  `/proc/1/stat` tick-delta agreed. A Grafana "Pod CPU (cores)" panel showed only
  ~4-core peaks for the same pod — believed to be rate-window **averaging of short
  bursts**, but reconcile this yourself (it was a live point of confusion).
- **Raising the BigTable channel pool 10 → 1000 did not change throughput.** Pool
  observed pinned at 1000, `channels_replaced` ~0 over a window. Interpreted as
  "channel-pool size is not the active limiter," but see the per-request limiter
  below before concluding.
- **A per-request BigTable semaphore exists**, capacity =
  `request_bigtable_concurrency` (code default 10, currently set to 100), in
  `bigtable_client.rs`. A unit test (`gate_stream_holds_permit_for_full_drain`)
  indicates a permit is held until its chunk's sub-stream **fully drains**, and
  the drain is consumer-paced. This is **per-request** (not obviously a
  cross-request serialization point) — don't overstate it.
- **One baseline probe** (config as in §4: concurrency=100, pool=1000, 4 gens ×
  50 VU = ~200 inflight, 120s window) recorded, as **window-deltas**:
  - completed `ListCheckpoints` ~**3.2/s**; mean end-to-end ~**61 s** (near the
    60 s timeout).
  - `permits_peak` for `list_checkpoints`: ~98% of requests exceeded 50; sum/count
    mean ~**83** (near the cap of 100).
  - `permit_wait_ms` mean by stage: `transactions` ~**3.8 s**, `objects`/
    `checkpoints` ~**0 ms**. i.e. wait concentrated at the transactions stage.
  - `bt_pool_rpcs_completed` ~**1510/s**; `ops_total` sum-rate ~**2000/s**.
  - This is **one run under one config** — not a curve, not causal.
- **Fixed-RPS behavior earlier in the session:** past a knee, requests queued and
  latency climbed into tens of seconds (deadline-bounded), rather than erroring —
  i.e. it appeared to degrade by latency/queue, not by a hard error flood.
  Reproduce before trusting.

---

## 8. Things the prior session got WRONG (so you don't repeat them)

- Initially attributed a 200-VU-from-2-generators collapse to server **OOM**, then
  to generator **OOMKill**. Both wrong: gens had `restarts=0`, no memory limit,
  60 GB node headroom. The `exit 137` was a **SIGKILL from the cleanup trap**, and
  k6 froze at high per-pod VU. Real fix was spreading VU across more gen pods.
- Repeatedly asserted "server is CPU-bound at ~110 obj-chunks/s/core" and "node
  contention is/ isn't the ceiling" — these flip-flopped as measurement methods
  changed. Do not inherit either claim; measure.
- Trusted `kubectl top node` and a Grafana panel as co-equal truth at various
  points; they disagreed with cAdvisor pod-level reads. Prefer **cAdvisor
  per-container** (`top pod --containers`) and prove any `/proc` reading targets
  the right process/cgroup before using it.

---

## 9. Open questions (neutral)

- Where does the latency actually accrue at the knee — acquiring a permit, holding
  it during downstream drain, BigTable round-trips, response rendering/
  serialization, tokio scheduling, or client-side draining (h2 flow control)? The
  metrics in §6 can decompose this if read as window-deltas.
- Is any limit **per-request** vs **cross-request/global**? (Determines whether
  concurrency multiplies or serializes fanout.)
- Does the configured `request_bigtable_concurrency` actually manifest as that many
  concurrent BigTable ops (`permits_peak` vs cap), or is effective concurrency
  lower (config wiring / picker / channel distribution)?
- Are the generators ever the limit at a given target, or the server? (Check gen
  CPU / dropped iterations / k6 warns per run.)
- Does BigTable itself have headroom during the knee (GCP-side CPU/QPS/errors)?

If you run a **tunable sweep** (e.g. varying `request-bigtable-concurrency` and/or
stage concurrency), change **one knob at a time** and hold everything else
identical — same stage config, pool size, pod resources, `dbg` presence, generator
split, request shape/checkpoint range, and window duration — and re-record the
**live config from the running pod** after each rollout. Otherwise runs aren't
comparable.

---

## 10. Gotchas

- **Copy k6 scripts to every generator before each run.** Rollouts wipe `/tmp`;
  missing scripts make a run silently under-deliver.
- **k6 env var names must match the script** (`HOST=`, not `H=`; k6 also only sees
  vars passed via `env ...` before `k6 run`, exposed as `__ENV`).
- **`MAX_RECV_MB`** must be large enough for worst-dense responses or streams
  error mid-run.
- **cAdvisor (`top pod --containers`) is the CPU source of truth**, not a Grafana
  rate panel (which averages short bursts) and not raw `/proc` until you've proven
  it reads the target process.
- **`rpc_inflight_requests` can go stale** if a k6 client dies without the server
  timing out the stream — it may read high while nothing is actually being served.
  Cross-check with completion counters.
- **Counters are cumulative since pod boot** and polluted by prior runs — always
  window-delta.
- **Metrics `:9184` isn't a declared containerPort** — reach it by pod IP /
  localhost (dbg sidecar or sibling pod), not via a Service.
- **zsh paste hazards** (harness runs interactive zsh): single-quote any argument
  containing `!` `[` `]` `*` `?` `{` `}` `$`; keep commands short (long lines wrap
  on paste and corrupt tokens); no inline `#` comments on command lines; write
  long JSON/script bodies to a file and reference them rather than inlining.
- **Generators saturate their own node** at high per-pod VU — scale out gen pods,
  don't crank one.

---

## 11. First moves for a fresh session

1. Re-verify live state: §4 re-check commands (server config, resources, node,
   dbg presence; generator fleet).
2. Read `bigtable_client.rs`, `pipeline.rs`, `config.rs`, and the
   `EXPERIMENT (nickv)` comments in `testnet-perftest.yaml` — understand the
   limiter/stage/drain model from source, not from §7.
3. Decide your own measurement plan for the objective in §1. §7 is a starting set
   of leads to confirm or refute, nothing more.
