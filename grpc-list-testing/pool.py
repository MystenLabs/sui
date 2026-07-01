# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Sample a LARGE spread pool of REAL filter values per (dimension, tier) from
Snowflake, for the LOAD harness.

Why this exists (hot-key avoidance):
  The *correctness* corpus (extract.py) deliberately picks the top-3 DENSEST
  values per tier (`rn <= 3 ORDER BY cnt DESC`) so counts are verifiable against
  an oracle. Replaying that handful as *load* is a synthetic hot-key generator:
  every VU hammers the same few BigTable row-key ranges, saturating one tablet
  server (and the block cache) while the cluster is idle -- you measure a
  hot-tablet limit, not per-replica capacity, and fail prematurely.

  This pool gives HUNDREDS of distinct real values per (dimension, tier) so a
  seeded picker (gen_load.py) spreads reads across the key space. Values are REAL
  (sampled from chain data), never synthetic: a random sender sent nothing ->
  empty-path, no index work (testing_plan.md sec 4, line 423-424).

Cost model:
  - `dense_everywhere` uses APPROX_TOP_K (single-pass sketch) -> cheap even over
    the full multi-billion-row table. These are the heavy-work keys that most
    need spreading.
  - `recent_only` / `sparse` are windowed GROUP BYs over a bounded checkpoint
    range (small scans).
  Results are cached by SQL hash (extract.snow_json), so re-runs are free.

Usage:
    python pool.py <net> [--per-tier=N] [--min-cnt=M] [--ceiling=N] [--dims=a,b]
    # e.g.  python pool.py mainnet --per-tier=300
Writes pool.<net>.json  (versioned artifact; regenerate ~monthly per the plan).
"""
from __future__ import annotations

import json
import sys
import time

import extract as ex  # reuse per-net config + snow_json cache (safe: main() is __main__-guarded)

PER_TIER = next((int(a.split("=", 1)[1]) for a in sys.argv[1:] if a.startswith("--per-tier=")), 200)
MIN_CNT = next((int(a.split("=", 1)[1]) for a in sys.argv[1:] if a.startswith("--min-cnt=")), 50)
_DIMS_ARG = next((a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--dims=")), None)

# dimension -> (table, value SQL expr, NOT NULL guard, extra WHERE)
_POOL_CFG = {
    "sender": ("TRANSACTION", "SENDER", "SENDER IS NOT NULL", ""),
    "move_call": ("MOVE_CALL", "PACKAGE || '::' || MODULE || '::' || FUNCTION_",
                  "PACKAGE IS NOT NULL AND MODULE IS NOT NULL AND FUNCTION_ IS NOT NULL", ""),
    "emit_module": ("EVENT", "PACKAGE || '::' || MODULE",
                    "PACKAGE IS NOT NULL AND MODULE IS NOT NULL", ""),
    "event_type": ("EVENT", "EVENT_TYPE", "EVENT_TYPE IS NOT NULL", ""),
    "affected_object": ("TRANSACTION_OBJECT", "OBJECT_ID", "OBJECT_ID IS NOT NULL",
                        f"AND OBJECT_STATUS IN {ex.CHANGED}"),
}

DIMS = (_DIMS_ARG.split(",") if _DIMS_ARG else list(_POOL_CFG))


def _dense_sql(table: str, valexpr: str, notnull: str, extra: str) -> str:
    # APPROX_TOP_K: single-pass frequent-items sketch. Returns ARRAY of [value, est_count].
    return (f"SELECT f.value[0]::string v, f.value[1]::int cnt "
            f"FROM (SELECT APPROX_TOP_K({valexpr}, {PER_TIER}, 100000) t "
            f"      FROM {ex.SCHEMA}.{table} "
            f"      WHERE CHECKPOINT < {ex.CEILING} AND {notnull} {extra}), "
            f"     LATERAL FLATTEN(input => t) f")


def _windowed_sql(table, valexpr, notnull, extra, lo, hi, cnt_lo, cnt_hi, order):
    having = f"COUNT(*) >= {cnt_lo}" if cnt_hi is None else f"COUNT(*) BETWEEN {cnt_lo} AND {cnt_hi}"
    return (f"SELECT {valexpr} v, COUNT(*) cnt, MIN(CHECKPOINT) lo, MAX(CHECKPOINT) hi "
            f"FROM {ex.SCHEMA}.{table} "
            f"WHERE CHECKPOINT BETWEEN {lo} AND {hi} AND {notnull} {extra} "
            f"GROUP BY 1 HAVING {having} ORDER BY cnt {order} LIMIT {PER_TIER}")


def _rows(sql: str) -> list[dict]:
    return [{k.lower(): v for k, v in r.items()} for r in ex.snow_json(sql)]


def sample_dim(dim: str) -> dict[str, list[dict]]:
    table, valexpr, notnull, extra = _POOL_CFG[dim]
    out: dict[str, list[dict]] = {}

    # dense_everywhere: frequent items across all history (band = full range).
    dense = _rows(_dense_sql(table, valexpr, notnull, extra))
    for r in dense:
        r.setdefault("lo", ex.GENESIS)
        r.setdefault("hi", ex.CEILING)
    out["dense_everywhere"] = dense

    # recent_only: values active in the shared recent window (band = [SHARED_LO, CEILING]).
    out["recent_only"] = _rows(_windowed_sql(
        table, valexpr, notnull, extra, ex.SHARED_LO, ex.CEILING, MIN_CNT, None, "DESC"))

    # sparse: low-frequency values in a recent sub-window (band = per-value [lo, hi]).
    out["sparse"] = _rows(_windowed_sql(
        table, valexpr, notnull, extra, ex.CEILING - 1_000_000, ex.CEILING, MIN_CNT, 500, "ASC"))

    return out


def main() -> None:
    print(f"net={ex.NET} schema={ex.SCHEMA} ceiling={ex.CEILING} shared_lo={ex.SHARED_LO} "
          f"per_tier={PER_TIER} min_cnt={MIN_CNT} dims={DIMS}")
    pool = {
        "net": ex.NET,
        "schema": ex.SCHEMA,
        "ceiling": ex.CEILING,
        "shared_lo": ex.SHARED_LO,
        "genesis": ex.GENESIS,
        "per_tier": PER_TIER,
        "min_cnt": MIN_CNT,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "dims": {},
    }
    for dim in DIMS:
        t0 = time.time()
        tiers = sample_dim(dim)
        pool["dims"][dim] = tiers
        counts = {k: len(v) for k, v in tiers.items()}
        print(f"  {dim:16} {counts}  ({time.time()-t0:.0f}s)")

    out_path = ex.HERE / f"pool.{ex.NET}.json"
    out_path.write_text(json.dumps(pool, indent=2))
    total = sum(len(v) for d in pool["dims"].values() for v in d.values())
    print(f"wrote {total} pooled values -> {out_path}")


if __name__ == "__main__":
    main()
