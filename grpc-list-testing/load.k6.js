// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// v2alpha List* LOAD script -- replays the hot-key-spread request list
// (load.<net>.jsonl from gen_load.py) under an open-loop arrival-rate ramp.
//
// Model A (testing_plan.md 2.6): ONE PAGE = ONE ITERATION, arrival-rate =
// pages/sec. Each iteration picks the NEXT pre-generated request (round-robin
// over the shuffled-by-seed list), so the value + checkpoint-start spread baked
// in by gen_load.py is what hits the server -- NO synthetic hot key.
//
// Why round-robin over the list (not random in-VU): the list is already a
// seeded, reproducible random sequence. Walking it deterministically keeps the
// run reproducible from the manifest and guarantees uniform pool coverage.
//
// Env:
//   HOST         target host:port     (e.g. kv-rpc-http2.rpc-kv-testnet.svc.cluster.local:8000)
//   REQ_FILE     which baked list to replay (default /data/load.mainnet.jsonl);
//                the image bakes one per net -> set /data/load.testnet.jsonl etc.
//   FLOOR        drop requests whose start_checkpoint < FLOOR (default 0).
//                Use the target's lowest_available_checkpoint when hitting a
//                PRUNED fullnode, so deep-history requests below its retained
//                window are skipped at runtime (no per-backend data regen).
//                0 = keep everything (BigTable archival serves full history).
//   PLAINTEXT    "1" for h2c (fullnode :9000); else TLS. kv-rpc :8000 is TLS
//                (gRPC-over-TLS; server RESETS a cleartext preface) -> leave
//                PLAINTEXT unset/0 for kv-rpc. insecureSkipTLSVerify (below) is
//                on because the cert has DNS SANs only (no IP SANs) so a pod-IP
//                target won't verify; internal one-off, skip is fine.
//   PROTO_ROOT   single dir holding the merged proto tree (default /proto):
//                sui/rpc/v2alpha/*, sui/rpc/v2/*, google/* under one root
//                (the two source roots are merged at image-build time; they do
//                not collide, so one import path suffices -- cf. §2.5's "two roots")
//   PROTO_FILE   entry proto, relative to PROTO_ROOT
//                (default sui/rpc/v2alpha/ledger_service.proto)
//   START_RPS,MAX_RPS,STEP_RPS,STEP_DUR,MAX_VUS   ramp knobs
//
// Run (per testing_plan.md 2.4: saturate ONE replica -> target a single pod IP,
// generator in-cluster, HPA disabled). The kv-rpc Service DNS is in the cert
// SANs, but a pod IP is not -> insecureSkipTLSVerify handles both. Single pod:
//   BK=$(kubectl -n <ns> get endpoints kv-rpc-http2 \
//        -o jsonpath='{.subsets[0].addresses[0].ip}:{.subsets[0].ports[0].port}')
//   k6 run -e HOST=$BK load.k6.js          # kv-rpc: TLS (PLAINTEXT unset)
//   k6 run -e HOST=<fullnode>:9000 -e PLAINTEXT=1 load.k6.js   # fullnode h2c

import grpc from 'k6/net/grpc';
import { SharedArray } from 'k6/data';
import { Trend, Counter, Rate } from 'k6/metrics';
import exec from 'k6/execution';

const HOST = __ENV.HOST || 'localhost:19000';
const REQ_FILE = __ENV.REQ_FILE || '/data/load.mainnet.jsonl';
const PLAINTEXT = __ENV.PLAINTEXT === '1';
const PROTO_ROOT = __ENV.PROTO_ROOT || '/proto';
const PROTO_FILE = __ENV.PROTO_FILE || 'sui/rpc/v2alpha/ledger_service.proto';
const FLOOR = Number(__ENV.FLOOR || 0); // drop requests starting below a pruned target's retained window
// Capacity-run knob: raise the client's max receive frame past the stock 4MB gRPC
// default so a busy checkpoint's ~4.66MB CheckpointItem doesn't ResourceExhaust
// mid-stream. UNSET = stock 4MB -- the ADOPTION-SIGNAL run (what an external builder
// on a default grpc-go/Java/Node client sees). Set MAX_RECV_MB=128 to match the
// first-party Sui SDK (sui-rpc-api client uses 128MiB) and measure SERVER capacity.
// Keep both runs: raised measures the server, stock measures real-client failure rate.
const MAX_RECV_MB = Number(__ENV.MAX_RECV_MB || 0);
const PRE_VUS = Number(__ENV.PRE_ALLOCATED_VUS || __ENV.MAX_VUS || 200); // prealloc, decoupled from maxVUs cap
const NSHARDS = Number(__ENV.NSHARDS || 1); // multi-generator: total shard count (horizontal scale-out)
const SHARD = Number(__ENV.SHARD || 0);     // this generator's shard index in [0, NSHARDS)

// One-page-per-iteration RPC map: each pre-gen line names its rpc.
const METHODS = {
  ListTransactions: 'sui.rpc.v2alpha.LedgerService/ListTransactions',
  ListEvents: 'sui.rpc.v2alpha.LedgerService/ListEvents',
  ListCheckpoints: 'sui.rpc.v2alpha.LedgerService/ListCheckpoints',
};

// SharedArray: parsed ONCE, shared across all VUs (not re-parsed per VU).
// FLOOR drops deep-history requests a pruned target can't serve (start below its
// retained window) -- lets one --floor=0 list drive both archival and pruned.
const reqs = new SharedArray('reqs', function () {
  let all = open(REQ_FILE).split('\n').filter((l) => l.length > 0).map((l) => JSON.parse(l));
  if (FLOOR) all = all.filter((r) => (r.request.start_checkpoint || 0) >= FLOOR);
  // Multi-generator sharding: pod SHARD of NSHARDS keeps a disjoint 1/N slice of
  // the (pre-shuffled) list -> each pod replays a representative, NON-overlapping
  // subset, so N pods don't hammer identical keys in lockstep (synthetic hot spots).
  if (NSHARDS > 1) all = all.filter((_, i) => i % NSHARDS === SHARD);
  return all;
});

const client = new grpc.Client();
client.load([PROTO_ROOT], PROTO_FILE);
// Connect ONCE per VU, then reuse the h2 connection across iterations (gRPC
// multiplexes each iteration's Stream over it). Connecting/closing per-iteration
// storms the server with TLS handshakes -> `connection reset by peer`.
let connected = false;
let errLogged = 0;                                 // cap raw error logging to first 5/VU (avoid log spam at high rps)

// Per-shape metrics (testing_plan.md 2.5: TTFF, drain, goodput; honest open-loop).
const ttff = new Trend('ttff_ms', true);          // time to first frame
const pageMs = new Trend('page_ms', true);        // full page latency
const items = new Trend('items_per_page');
const goodput = new Counter('pages_ok');
const errRate = new Rate('page_errors');
// Categorize stream errors so the next run says WHAT failed (deadline vs reset vs
// GOAWAY vs cancel), not just "1% errored". k6 gRPC error carries {code,message}.
const errByKind = new Counter('page_errors_by_kind');

export const options = {
  scenarios: {
    ramp: {
      executor: 'ramping-arrival-rate',           // OPEN-LOOP (2.1/decision 5)
      startRate: Number(__ENV.START_RPS || 2),
      timeUnit: '1s',
      preAllocatedVUs: PRE_VUS,
      maxVUs: Number(__ENV.MAX_VUS || 200),
      stages: buildStages(),
    },
  },
  thresholds: {                                    // surface metrics in the summary
    page_ms: ['p(95)>=0'],
    ttff_ms: ['p(95)>=0'],
    page_errors: ['rate>=0'],
  },
  insecureSkipTLSVerify: true, // kv-rpc :8000 + fullnode :9443 both self-signed / SAN-less for pod-IP targets
};

function buildStages() {
  const start = Number(__ENV.START_RPS || 2);
  const max = Number(__ENV.MAX_RPS || 32);
  const step = Number(__ENV.STEP_RPS || 2);
  const dur = __ENV.STEP_DUR || '30s';
  const stages = [];
  for (let r = start; r <= max; r += step) stages.push({ target: r, duration: dur });
  return stages;
}

// Normalize a k6 gRPC stream error to a SMALL, fixed set of categories (low
// cardinality -- safe as a metric tag at high rps). k6 passes {code,message};
// code is the gRPC status int. Connection-level failures have no code -> inspect
// the message. Anything unrecognized -> 'other' (never free-form -> no tag blowup).
function errKind(e) {
  const code = e && typeof e.code === 'number' ? e.code : null;
  if (code === 4) return 'deadline';            // DEADLINE_EXCEEDED
  if (code === 1) return 'cancelled';           // CANCELLED (client abort)
  if (code === 8) return 'resource_exhausted';  // rate-limit / overload
  if (code === 14) return 'unavailable';        // conn refused / GOAWAY / drain
  if (code === 13) return 'internal';           // server internal
  const m = (e && e.message ? String(e.message) : '').toLowerCase();
  if (m.includes('deadline')) return 'deadline';
  if (m.includes('reset') || m.includes('eof') || m.includes('goaway') || m.includes('connection')) return 'conn_reset';
  return 'other';
}

export default function () {
  // Round-robin the shuffled list. iterationInTest is a GLOBAL monotonic counter
  // across ALL VUs in the scenario (unlike __ITER, which is per-VU -> at high VU
  // counts every VU would replay reqs[0],reqs[1],... in lockstep = synthetic hot
  // spots + warm-cache readings that understate real cost). Global -> uniform coverage.
  const rec = reqs[exec.scenario.iterationInTest % reqs.length];
  const method = METHODS[rec.rpc];
  // Connect once per VU, then reuse across iterations. On any connect failure or
  // mid-run stream error (idle GOAWAY, LB drop, a backend rollout) we reset
  // `connected` in the error path below, so the NEXT iteration reconnects rather
  // than pinning the VU to a dead h2 connection for the rest of the run.
  if (!connected) {
    try {
      const connectParams = { plaintext: PLAINTEXT, timeout: '10s' };
      if (MAX_RECV_MB > 0) connectParams.maxReceiveSize = MAX_RECV_MB * 1024 * 1024;
      client.connect(HOST, connectParams);
      connected = true;
    } catch (e) {
      errRate.add(true);                             // count the failed attempt; retry next iter
      errByKind.add(1, { kind: errKind(e) });
      if (errLogged < 5) { errLogged++; console.log(`connect-err kind=${errKind(e)} code=${e && e.code} msg=${(e && e.message || '').slice(0,120)}`); }
      client.close();
      return;
    }
  }

  const t0 = Date.now();
  let n = 0;
  let firstFrame = 0;
  let settled = false;                             // 2.6 gotcha: on('error')+on('end') both fire

  const stream = new grpc.Stream(client, method);
  stream.on('data', function (msg) {
    if (!firstFrame) { firstFrame = Date.now(); ttff.add(firstFrame - t0); }
    if (msg && msg.item) n += 1;                   // count only item frames (skip watermark/end)
  });
  stream.on('error', function (e) {
    if (settled) return; settled = true;
    errRate.add(true); pageMs.add(Date.now() - t0);
    const kind = errKind(e);
    errByKind.add(1, { kind });                      // low-cardinality category tag
    if (errLogged < 5) { errLogged++; console.log(`stream-err kind=${kind} code=${e && e.code} msg=${(e && e.message || '').slice(0,120)}`); }
    // conn may be dead (reset/GOAWAY). Drop it -> fresh connect next iteration,
    // else every later page on this VU fails. (deadline/cancel don't kill the
    // conn, but reconnecting is cheap vs. mis-pinning a dead one.)
    connected = false;
    client.close();
  });
  stream.on('end', function () {
    if (settled) return; settled = true;
    errRate.add(false); goodput.add(1);
    items.add(n); pageMs.add(Date.now() - t0);
    // NB: do NOT client.close() on success -- the connection is reused across
    // iterations (connect-once above). Closing per-page storms the server with
    // reconnects and triggers `connection reset by peer`.
  });

  stream.write(rec.request);                       // protojson request, verbatim from gen_load
  stream.end();
}
