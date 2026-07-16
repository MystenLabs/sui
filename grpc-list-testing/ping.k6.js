// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// Throwaway connectivity preflight: ONE light ListCheckpoints (limit 1, seq only).
// A healthy kv-rpc answers this near-instantly. Gate the ramp on PREFLIGHT-OK.
import grpc from 'k6/net/grpc';
export const options = { insecureSkipTLSVerify: true };
const HOST = __ENV.HOST || 'localhost:8000';
const PLAINTEXT = __ENV.PLAINTEXT === '1';
const PROTO_ROOT = __ENV.PROTO_ROOT || '/proto';
const PROTO_FILE = __ENV.PROTO_FILE || 'sui/rpc/v2/ledger_service.proto';
const c = new grpc.Client();
c.load([PROTO_ROOT], PROTO_FILE);
export default function () {
  try {
  c.connect(HOST, { plaintext: PLAINTEXT, timeout: '5s' });
  } catch (e) {
    console.log('CONNECT-FAIL ' + (e && e.message));
    return;
  }
  const s = new grpc.Stream(c, 'sui.rpc.v2.LedgerService/ListCheckpoints', { timeout: '8s' });
  let got = false;
  s.on('data', function () {
    if (got === false) { got = true; console.log('PREFLIGHT-OK reachable+answering'); }
    c.close();
  });
  s.on('error', function (e) {
    got = true;
    console.log('PREFLIGHT-ERR code=' + (e && e.code) + ' ' + ((e && e.message) || '').slice(0, 80));
  });
  s.write({ options: { limit: 1 }, read_mask: 'sequenceNumber' });
  s.end();
}
