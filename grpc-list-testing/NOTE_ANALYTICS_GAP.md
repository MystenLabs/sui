<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Analytics `MOVE_CALL` data gap (testnet) — and why the shared window stops at 342M

## TL;DR
The Snowflake analytics warehouse (`CHAINDATA_TESTNET`) is **missing a contiguous batch
of `MOVE_CALL` rows** around checkpoints **342,206,316 – 342,208,925** (~2,600 cp). The
kv-rpc / bitmap index is **correct** (chain-verified). To keep the corpus oracle off the
bad data, `extract.py` caps the testnet **shared** exact-count window at `shared_hi =
342,000,000` (below the gap). Archival-only cases still span `[0, CEILING]` but are
capped → structural-only, so the gap doesn't produce false FAILs there.

This is **data quality in the analytics indexer**, not an RPC bug and not a test bug.
Filed here so it can be picked up when analytics is backfilled; until then the corpus
routes around it.

## Evidence
- **Differential:** `ListTransactions(move_call=0x2::coin::destroy_zero)` over the shared
  window returns **+9** vs Snowflake (16,212 vs 16,203); `ListCheckpoints` shows the same
  +9 (7,750 vs 7,741) after the ListCheckpoints under-return fix.
- **Chain-confirmed (independent fullnode `GetTransaction`):** all 9 RPC-only digests
  genuinely call `0x2::coin::destroy_zero` (multi-command Walrus blob PTBs). `RPC_only=9,
  SF_only=0` → RPC is a strict superset; Snowflake is missing them.
- **Snowflake-internal (diag):** over `[342,200,000, 342,215,000]`, `TRANSACTION` has
  15,001 distinct checkpoints but `MOVE_CALL` has only 6,282; `MOVE_CALL` rows stop at cp
  342,206,315 and resume at 342,208,926 — a contiguous zero-`MOVE_CALL` band. 2,600
  consecutive checkpoints with no move calls is implausible (move calls are ubiquitous) and
  the chain proves txns in that band do make move calls → a dropped/un-written batch.

## Open questions (not yet resolved)
- **Code bug vs operational gap:** either the `MOVE_CALL` pipeline's watermark advanced past
  a checkpoint range whose rows weren't durably written (silent loss), or a backfill batch
  failed to upload and wasn't retried. Distinguish by reading the indexer's watermark/commit
  records and checking whether other derived tables (`EVENT`, `TRANSACTION_OBJECT`) also gap
  at the same band.
- **May self-heal:** if it's an ops gap, analytics re-processing `[342.2M]` would close it —
  re-run the re-verification below to check.
- **Magnitude:** +9 is just the `destroy_zero` slice; the band is missing thousands of
  `MOVE_CALL` rows across all functions.
- **Possible second micro-gap:** the `sender` dimension showed a small +4 over the shared
  window sourced from the `TRANSACTION` table (which looked checkpoint-complete). Not
  located; capping at 342M may or may not cover it — the re-run reveals it.

## Re-verify (cheap)
- `diag.sql` / `gap.sql` (this dir): re-run against Snowflake to check if the band still
  has the `MOVE_CALL` hole.
- `correctness/arbiter.py` with `--chain-target <fullnode>`: re-confirms the move_call deltas
  are still RPC-only and chain-real.

## If/when fixing analytics
Suspect code: `crates/sui-analytics-indexer/src/handlers/tables/move_call.rs` (the
`MoveCallProcessor`) plus its watermark/commit/upload path. Backfill the missing band and add
a regression/self-check that `MOVE_CALL` coverage tracks `TRANSACTION` over move-call-bearing
checkpoints. Once backfilled, raise `testnet.shared_hi` back to the ceiling in `extract.py`.
