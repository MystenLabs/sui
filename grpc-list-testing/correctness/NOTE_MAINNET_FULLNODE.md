<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mainnet v2alpha fullnode correctness — method + findings (2026-06-30)

Mainnet kv-rpc archival isn't backfilled yet, so there is no cross-backend
differential. Instead we test the **v2alpha fullnode** directly as the harness's
primary backend and count-check against the Snowflake `CHAINDATA_MAINNET` oracle,
which is **independent** of the RPC (not RPC-derived) — arguably a stronger check
than the testnet fullnode-vs-kv-rpc differential (which is metamorphic).

## Backend
- Fullnode: `sui-node-mainnet-rpc-alpha.rpc-mainnet:9000` (plaintext h2c; the
  `:9443` TLS port has a SAN-less cert that gRPC rejects).
- chain_id `4btiuiMPvEENsttpZC7CZ53DruC3MAgfznDbASZ7DR6S` (mainnet), server
  `sui-node/1.75.0-f8ad4781881d`.

## The window problem (and fix)
The fullnode **prunes**. `GetServiceInfo` reported served window
`[285,592,119 → 293,135,740]` (~7.5M checkpoints); the floor rises over time.
Snowflake `CHAINDATA_MAINNET` covers `[0 → 293,130,793]`. The corpus window must
sit inside **both**. The old mainnet corpus (ceiling 288M, range `[0,288M]`) is
almost entirely below the fullnode floor — useless here.

Fix: `extract.py` mainnet retargeted to **ceiling 293M / window 6M → shared
`[287M, 293M]`** (1.4M margin above the prune floor), and a new `--genesis=N`
override makes the "archival" range = the retained window instead of true
genesis. Regenerate with:

```sh
python3 ../extract.py mainnet --genesis=287000000
```

This windows every query (fast; reuses cached shared oracles) and makes all but
the 2 genesis-boundary probes (`edge_genesis`, `recent_over_genesis`) servable.

## Run
```sh
kubectl -n rpc-mainnet port-forward svc/sui-node-mainnet-rpc-alpha 19000:9000 &
uvx --with grpcio --with protobuf --with googleapis-common-protos python harness.py \
    --corpus ../corpus.mainnet.jsonl --archival localhost:19000 \
    --only '^(?!.*(edge_genesis|recent_over_genesis))' \
    --max-drain 80000 --partial-pages 4 --no-diff --out results.mainnet.json
```

## Result: `PASS=100  FAIL=2  SKIP=67` (152 cases / 169 checks)
- **Zero count-mismatches.** 81 cases are real exact-count matches (fullnode
  result == Snowflake), e.g. `sender.recent_only` 65,097==65,097,
  `emit_module.recent_only` 29,874==29,874. The other 19 PASS are
  structural/invariant. The fullnode agrees with the independent oracle on every
  drainable case.
- **67 SKIP** = capped giants (mainnet density pushes many exact counts past the
  80k `--max-drain` cap → structural-only). Expected; raise `--max-drain` to verify.

## FINDING: `INTERNAL` on bitmap scan-budget exhaustion (2 cases, deterministic)

Two complex-DNF cases fail with a hard gRPC error rather than a graceful result:

```
StatusCode.INTERNAL: 2 concurrent errors
  [0] bitmap scan budget exhausted
  [1] bitmap scan budget exhausted
```

- `tx.sender_not_move_call.anchored.shared` — `sender(0x8af2…) AND NOT
  move_call(…return_flashloan_quote)` over `[287M,293M]`.
- `tx.degenerate.dense_and_dense.empty.archival` — `sender(0x0) AND
  move_call(…vaa::parse_and_verify)` over `[287M,293M]` (0x0 system sender is
  enormously dense).

**Reproducible 3/3** in isolation (`_mfail.py`). These same case shapes drained
cleanly on the *testnet* fullnode (e.g. `sender_not_move_call` → 8,822 items
`[ok]`); the failure is **mainnet-density-triggered**: each conjunction literal's
bitmap scan is individually huge, and the two parallel scans ("2 concurrent
errors") both blow the per-query bitmap scan budget.

**Why this looks like a real robustness issue (not just "query too big"):**
budget exhaustion is an *expected* condition for adversarial filters, and the
List APIs already have a graceful, resumable terminal for it —
`QUERY_END_REASON_SCAN_LIMIT` (client paginates past it). Returning a hard
`INTERNAL` instead is (a) non-resumable — the client can't make progress — and
(b) indistinguishable from a genuine server fault. Recommend the API team decide
whether scan-budget exhaustion on multi-literal conjunctions should surface as a
resumable `SCAN_LIMIT`/`RESOURCE_EXHAUSTED` rather than `INTERNAL`.

Repro: `_mfail.py` (loads the 2 corpus cases, drains each 3×). Served-window
probe: `_fninfo.py`. Both use a plaintext port-forward to the mainnet fullnode.
