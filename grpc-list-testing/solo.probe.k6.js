// Solo worst-case ListCheckpoints probe. 1 VU, sequential, no load.
// Fires the single most expensive ListCheckpoints record from the corpus and
// CAPTURES the gRPC code+message per iteration (the deployed load.k6.js throws
// the error object away). Discriminator: fast-alone => under-load errors are
// contention; errors-alone => intrinsic (deadline / 4MB frame).
import grpc from 'k6/net/grpc';
import { Trend, Counter } from 'k6/metrics';

const HOST = __ENV.HOST || 'localhost:8000';
const PLAINTEXT = __ENV.PLAINTEXT === '1';
const PROTO_ROOT = __ENV.PROTO_ROOT || '/proto';
const PROTO_FILE = __ENV.PROTO_FILE || 'sui/rpc/v2/ledger_service.proto';
const REQ_FILE = __ENV.REQ_FILE || '/data/load.testnet.jsonl';
const ITERS = Number(__ENV.ITERS || 30);
// See load.k6.js: UNSET = stock 4MB (adoption signal); MAX_RECV_MB=128 matches the
// first-party Sui SDK to measure server capacity past a large checkpoint body.
const MAX_RECV_MB = Number(__ENV.MAX_RECV_MB || 0);

const HEAVY = /contents|transactions|objects|summary|signature/i;
function span(r) { return (r.request.end_checkpoint || 0) - (r.request.start_checkpoint || 0); }
function heavy(r) { return HEAVY.test(r.request.read_mask || '') ? 1 : 0; }

// Rank once: worst ListCheckpoints = heaviest mask, then widest span,
// preferring the dense_everywhere tier (dense sender => matches across all history).
const worst = (function () {
  const all = open(REQ_FILE).split('\n').filter((l) => l.length > 0).map((l) => JSON.parse(l));
  const lc = all.filter((r) => r.rpc === 'ListCheckpoints');
  lc.sort((a, b) => {
    const ta = a.tier === 'dense_everywhere' ? 1 : 0, tb = b.tier === 'dense_everywhere' ? 1 : 0;
    return (heavy(b) - heavy(a)) || (tb - ta) || (span(b) - span(a));
  });
  return lc[0];
})();

const ttff = new Trend('solo_ttff_ms', true);
const dur = new Trend('solo_dur_ms', true);
const okc = new Counter('solo_ok');
const errc = new Counter('solo_err');

const client = new grpc.Client();
client.load([PROTO_ROOT], PROTO_FILE);

export const options = {
  insecureSkipTLSVerify: true, // pod-IP self-signed / SAN-less
  scenarios: { solo: { executor: 'per-vu-iterations', vus: 1, iterations: ITERS, maxDuration: '5m' } },
};

export function setup() {
  console.log(`WORST ListCheckpoints: tier=${worst.tier} dim=${worst.dim} ` +
    `read_mask="${worst.request.read_mask}" span=${span(worst)} ` +
    `limit=${worst.request.options ? worst.request.options.limit : '?'}`);
  return {};
}

let connected = false;

export default function () {
  // Mirror the PROVEN deployed load.k6.js pattern: connect-once, NO sleep/wait
  // loop, close only on error, and RETURN so k6 drains the stream post-return
  // (that drain fires the callbacks; the earlier busy-loop suppressed them ->
  // the 15s TIMEOUT). Only addition vs deployed: capture the error arg.
  if (!connected) {
    try {
      const connectParams = { plaintext: PLAINTEXT, timeout: '15s' };
      if (MAX_RECV_MB > 0) connectParams.maxReceiveSize = MAX_RECV_MB * 1024 * 1024;
      client.connect(HOST, connectParams); connected = true;
    }
    catch (e) { errc.add(1); console.log(`CONNECT-ERR msg="${e && e.message}"`); client.close(); connected = false; return; }
  }
  const t0 = Date.now();
  let n = 0, first = 0, done = false;
  const stream = new grpc.Stream(client, 'sui.rpc.v2.LedgerService/ListCheckpoints');
  stream.on('data', (msg) => { if (!first) { first = Date.now(); ttff.add(first - t0); } if (msg && msg.checkpoint) n += 1; });
  stream.on('error', (e) => {
    if (done) return; done = true;
    errc.add(1); dur.add(Date.now() - t0);
    console.log(`ERR code=${e && e.code} msg="${e && e.message}" items=${n} dur=${Date.now() - t0}ms`);
    connected = false; client.close();          // drop dead h2, reconnect next iter
  });
  stream.on('end', () => {
    if (done) return; done = true;
    okc.add(1); dur.add(Date.now() - t0);
    console.log(`OK items=${n} ttff=${first ? first - t0 : -1}ms dur=${Date.now() - t0}ms`);
  });
  stream.write(worst.request);
  stream.end();
}
