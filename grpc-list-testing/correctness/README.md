<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# v2alpha LedgerService correctness harness

Replays a corpus (`../corpus.<net>.jsonl`, built by `../extract.py`) against a
kv-rpc archival endpoint and/or a fullnode, drains each server stream to
completion, and checks every case against its oracle. **Bespoke, not k6** — k6
is the load generator; correctness needs full-drain + set algebra + a
cross-backend diff, which is the opposite shape from a load tool.

## What it checks

| oracle / invariant | check |
|---|---|
| `exact_count` | `|result set| == expected_count` (Snowflake oracle) |
| `decomposition` | set algebra over component cases: `union` / `difference` |
| `degenerate` | bounded single-page probe: 0 items + expected terminal reason |
| tiling | no duplicate identity across pages |
| watermark | `checkpoint_hi` ↑ (asc) / `checkpoint_lo` ↓ (desc), monotonic |
| ordering | `asc result == reverse(desc result)` for paired cases |
| read_mask | cheap/heavy twins return the same identity set |
| differential | archival vs fullnode identity sets equal (`shared`-scope only) |

Requests are parsed straight from the corpus `request` (canonical protojson)
into the generated proto types via `json_format` — one source of truth, no
bespoke filter builder. `--list` also makes this a full offline corpus
validation (every request must parse against the pinned proto rev).

## Files

- `harness.py` — the harness (drain + oracle + invariants + differential).
- `test_harness.py` — unit tests (fake stream, no network). `uvx --with grpcio --with protobuf --from pytest pytest test_harness.py`
- `gen_stubs.sh` — regenerates `sui_pb/` from the pinned `sui-rpc` protos.
- `sui_pb/` — generated stubs (gitignored; run `gen_stubs.sh`).
- `Dockerfile`, `k8s-job.yaml.template` — in-cluster one-off Job.

## Setup

```sh
./gen_stubs.sh        # generates sui_pb/ from the pinned sui-rpc proto rev
```

Runtime needs `grpcio` + `protobuf`. With uv:

```sh
uvx --with grpcio --with protobuf python harness.py --corpus ../corpus.testnet.jsonl --list
```

## Running

### Local, via port-forward (recommended for manual / iteration)

Correctness is **low-rate**, so the kube-apiserver port-forward tunnel is fine
here (unlike the saturation/load test, which must run in-cluster against a
single pod). One terminal:

The production testnet kv-rpc serves **TLS h2 (self-signed)** on :8000 — cert
`CN=sui-node-rpc.svc.cluster.local`, SAN `kv-rpc-http2.rpc-kv-testnet.svc.cluster.local`.
(A plaintext testbed is the exception, not the deployed service.) So you pin the
server cert as the CA and override the TLS authority to a SAN over the
`localhost` forward.

Grab the cert once:

```sh
kubectl -n rpc-kv-testnet port-forward svc/kv-rpc-http2 18000:8000 &   # transient
openssl s_client -connect localhost:18000 -alpn h2 </dev/null 2>/dev/null \
  | openssl x509 -outform PEM > kvrpc.testnet.crt
```

`kubectl port-forward` exits when its target pod cycles, so **supervise it** for
a long drain; the harness retries transient `UNAVAILABLE` and resumes from the
last cursor, bridging restarts:

```sh
CTX=gke_workloads-primary_us-east4_workloads-primary-use4
( while true; do kubectl --context "$CTX" port-forward -n rpc-kv-testnet \
    svc/kv-rpc-http2 18000:8000; sleep 2; done ) &

uvx --with grpcio --with protobuf python harness.py \
    --corpus ../corpus.testnet.jsonl \
    --archival localhost:18000 --archival-tls --archival-ca kvrpc.testnet.crt \
    --archival-server-name kv-rpc-http2.rpc-kv-testnet.svc.cluster.local \
    --max-drain 80000 --no-diff --out results.testnet.json
```

Add the cross-backend differential against the testnet fullnode. It serves the
v2alpha List APIs and retains the shared window (verified: `ListCheckpoints`/
`ListTransactions`/`ListEvents` all served at cp 334M–344.2M). Use the **plaintext
h2c** endpoint `sui-node-rpc-alpha.rpc-testnet:9000` — the `:9443` TLS port has a
SAN-less cert (`CN`-only) that gRPC's hostname verification rejects.

```sh
( while true; do kubectl --context "$CTX" port-forward -n rpc-testnet \
    svc/sui-node-rpc-alpha 19000:9000; sleep 2; done ) &

# drop --no-diff, add the fullnode (plaintext); differential runs on shared-scope cases
... --fullnode localhost:19000 --fullnode-insecure --only 'shared'
```

The differential drains each shared case on both backends and asserts identical
id-sets (exact-reverse for asc/desc). Two limits: (1) it only covers the **shared
window** — the fullnode prunes deep history, so archival-only cases stay kv-rpc-vs-
Snowflake; (2) it is **metamorphic**, not independent — both backends are derived
views, so a bug in shared `sui-rpc-api` serving code passes identically on both.
The chain (`GetTransaction`, via the arbiter) remains the independent tiebreaker.

Useful flags: `--only '<regex>'` (subset by id), `--max-drain N` (see below),
`--raw-mask` (drain each case's own read_mask instead of the identity mask),
`--out results.json`, `--no-diff`, `--timeout`.

### In-cluster (one-off Job)

```sh
./gen_stubs.sh
docker build -f Dockerfile -t <registry>/ledger-correctness:<tag> ..   # context = grpc-list-testing/
# push, fill in k8s-job.yaml.template -> k8s-job.yaml
kubectl apply -f k8s-job.yaml
kubectl -n <ns> logs -f job/ledger-correctness
kubectl -n <ns> delete job ledger-correctness
```

## Deployment: don't make a Pulumi stack (yet)

The `sui-kv-rpc` Pulumi program is long-lived infra (the **system under test**).
A correctness run is a transient run-to-completion job — it does **not** belong
in a desired-state IaC stack (Pulumi would treat a finished Job as drift). Keep
tester and SUT separate:

1. **Now (manual, this week):** port-forward + run locally. Zero cluster objects.
2. **In-cluster ad hoc:** the one-off `kubectl apply` Job above. Apply, read
   logs, delete.
3. **Later (scheduled CI):** *then* a small Pulumi stack with a **CronJob**
   (mirror the §5 `simulator-nightly` pattern), reusing the kv-rpc program's
   `getProviderFromESC` provider — a separate stack from the service.

Hitting the load-balanced **Service** (not a single pod) is correct here:
every request is bounded by `end_checkpoint ≤ CP_CEILING`, and history below a
final checkpoint is immutable, so any archival pod returns identical results.

## Caveats

- **`--max-drain` (default 250k).** `exact_count` cases whose total exceeds the
  cap (e.g. the `0x0` system sender, `unfiltered` archival — up to 3.6B) are
  **partial-checked**: structural invariants only, count reported `UNVERIFIED`,
  result `SKIP`. Decompositions whose primary/components exceed the cap also
  `SKIP`. Raise `--max-drain` to verify them (slower). The bulk of correctness
  signal (shared-window cases ≤ cap, all edges, most decompositions) verifies
  fully at the default.
- **Cluster tip.** The testnet corpus assumes the kv-rpc cluster is backfilled
  to `CP_CEILING` (350M; see `../manifest.testnet.json`). `shared`/`recent`
  cases over `[340M, 350M]` need that range present, or they read short.
- **Proto rev.** `sui_pb/` must match the corpus `proto_rev`. Re-run
  `gen_stubs.sh` after any bump.
