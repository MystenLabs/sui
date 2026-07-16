// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// Fixed-shape concurrency probe: fires the SINGLE worst-dense full-mask
// ListCheckpoints (identical to solo.probe) but under a constant-vus closed loop
// so ONLY concurrency varies between runs. Purpose: isolate load-induced
// contention (solo VUS=1 vs load VUS=N) on an identical request, since server
// metrics are labeled by method/stage only (no shape/read_mask label) -> the
// shape MUST be held constant for a valid solo->load per-stage delta.
import grpc from 'k6/net/grpc';
import { Trend, Counter } from 'k6/metrics';

const HOST = __ENV.HOST || 'localhost:8000';
const PLAINTEXT = __ENV.PLAINTEXT === '1';
const PROTO_ROOT = __ENV.PROTO_ROOT || '/proto';
const PROTO_FILE = __ENV.PROTO_FILE || 'sui/rpc/v2/ledger_service.proto';
const REQ_FILE = __ENV.REQ_FILE || '/data/load.testnet.jsonl';
const MAX_RECV_MB = Number(__ENV.MAX_RECV_MB || 0);
const VUS = Number(__ENV.VUS || 1);
const DUR = __ENV.DUR || '60s';

const HEAVY = /contents|transactions|objects|summary|signature/i;
function span(r) { return (r.request.end_checkpoint || 0) - (r.request.start_checkpoint || 0); }
function heavy(r) { return HEAVY.test(r.request.read_mask || '') ? 1 : 0; }

// Rank once: worst ListCheckpoints = heaviest mask, widest span, dense_everywhere.
const worst = (function () {
  const all = open(REQ_FILE).split('\n').filter((l) => l.length > 0).map((l) => JSON.parse(l));
  const lc = all.filter((r) => r.rpc === 'ListCheckpoints');
  lc.sort((a, b) => {
    const ta = a.tier === 'dense_everywhere' ? 1 : 0, tb = b.tier === 'dense_everywhere' ? 1 : 0;
    return (heavy(b) - heavy(a)) || (tb - ta) || (span(b) - span(a));
  });
  return lc[0];
})();

const ttff = new Trend('fx_ttff_ms', true);
const dur = new Trend('fx_dur_ms', true);
const okc = new Counter('fx_ok');
const errc = new Counter('fx_err');

const client = new grpc.Client();
client.load([PROTO_ROOT], PROTO_FILE);

export const options = {
  insecureSkipTLSVerify: true,
  scenarios: { fixed: { executor: 'constant-vus', vus: VUS, duration: DUR } },
};

export function setup() {
  console.log(`FIXED worst ListCheckpoints: tier=${worst.tier} dim=${worst.dim} ` +
    `read_mask="${worst.request.read_mask}" span=${span(worst)} ` +
    `limit=${worst.request.options ? worst.request.options.limit : '?'} VUS=${VUS} DUR=${DUR}`);
  return {};
}

let connected = false;

export default function () {
  if (!connected) {
    try {
      const cp = { plaintext: PLAINTEXT, timeout: '15s' };
      if (MAX_RECV_MB > 0) cp.maxReceiveSize = MAX_RECV_MB * 1024 * 1024;
      client.connect(HOST, cp); connected = true;
    } catch (e) { errc.add(1); client.close(); connected = false; return; }
  }
  const t0 = Date.now();
  let n = 0, first = 0, done = false;
  const stream = new grpc.Stream(client, 'sui.rpc.v2.LedgerService/ListCheckpoints');
  stream.on('data', (msg) => { if (!first) { first = Date.now(); ttff.add(first - t0); } if (msg && msg.checkpoint) n += 1; });
  stream.on('error', (e) => {
    if (done) return; done = true;
    errc.add(1); dur.add(Date.now() - t0);
    connected = false; client.close();
  });
  stream.on('end', () => {
    if (done) return; done = true;
    okc.add(1); dur.add(Date.now() - t0);
  });
  stream.write(worst.request);
  stream.end();
}
