# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Generate a DETERMINISTIC, hot-key-spread request list for the LOAD harness.

Consumes pool.<net>.json (pool.py) and emits load.<net>.jsonl: one protojson
List{Transactions,Events,Checkpoints} request per line, exactly what k6 replays
(no translation). A seeded PRNG makes the whole sequence reproducible from the
manifest -- per testing_plan.md sec v0: "generate the request list ahead of time
from the seed so the run is deterministic and the manifest fully reproduces it."

Hot-key avoidance -- randomize BOTH axes of the BigTable row key per iteration:
  1. filter VALUE   -> uniform pick over a large pool (spreads the value prefix).
     Uniform, NOT cnt-weighted: weighting by frequency re-concentrates on the
     densest keys, recreating the hot spot we're trying to avoid.
  2. checkpoint START -> random within each value's active band (spreads the
     checkpoint-bucket suffix, so scans don't all begin at the same row).

One request = one page (Model A, testing_plan.md sec 2.6): k6 arrival-rate =
pages/sec, so each line is a single ListX with limit=<page>. The scan walks
buckets from a random start -> honest per-page server work, spread across keys.

Usage:
    python gen_load.py <net> [--n=20000] [--seed=1] [--page=50] [--heavy=0.1] \
                             [--rpc-mix=..] [--tier-mix=..]
    # e.g.  python gen_load.py mainnet --n=50000 --seed=42
Writes load.<net>.jsonl + load_manifest.<net>.json (sidecar).
"""
from __future__ import annotations

import json
import random
import sys
import time
from pathlib import Path

import corpus_builder as cb

HERE = Path(__file__).parent


def _arg(name, default, cast=str):
    for a in sys.argv[1:]:
        if a.startswith(f"--{name}="):
            return cast(a.split("=", 1)[1])
    return default


def _mix(s: str) -> dict[str, float]:
    """Parse 'a:0.5,b:0.3' -> {a:0.5,b:0.3}."""
    out = {}
    for part in s.split(","):
        k, v = part.split(":")
        out[k.strip()] = float(v)
    return out


NET = next((a for a in sys.argv[1:] if not a.startswith("-")), "testnet")
N = int(_arg("n", 20000, int))
SEED = int(_arg("seed", 1, int))
PAGE = int(_arg("page", 50, int))
FLOOR = int(_arg("floor", 0, int))  # clamp start_checkpoint >= FLOOR (target's served floor;
#                                     0 = full-history archival/BigTable; ~285.6M = pruning fullnode)
HEAVY_FRAC = float(_arg("heavy", 0.1, float))
RPC_MIX = _mix(_arg("rpc-mix", "ListTransactions:0.5,ListEvents:0.3,ListCheckpoints:0.2"))
TIER_MIX = _mix(_arg("tier-mix", "dense_everywhere:0.5,recent_only:0.3,sparse:0.2"))

# dimensions valid per RPC (mirrors corpus_builder._PREDICATES_FOR_RPC / extract TX_DIMS,EV_DIMS)
TX_DIMS = ["sender", "move_call", "emit_module", "event_type", "affected_object"]
EV_DIMS = ["sender", "emit_module", "event_type"]
RPC_DIMS = {"ListTransactions": TX_DIMS, "ListCheckpoints": TX_DIMS, "ListEvents": EV_DIMS}

# dimension -> predicate constructor (from corpus_builder; all emit 32-byte-padded protojson)
PREDICATE = {
    "sender": cb.sender,
    "move_call": cb.move_call,
    "emit_module": cb.emit_module,
    "event_type": cb.event_type,
    "affected_object": cb.affected_object,
}


def weighted(rng: random.Random, weights: dict[str, float], allowed=None) -> str:
    items = [(k, w) for k, w in weights.items() if (allowed is None or k in allowed) and w > 0]
    total = sum(w for _, w in items)
    x = rng.random() * total
    for k, w in items:
        x -= w
        if x <= 0:
            return k
    return items[-1][0]


def main() -> None:
    pool_path = HERE / f"pool.{NET}.json"
    if not pool_path.exists():
        raise SystemExit(f"missing {pool_path} -- run: python pool.py {NET}")
    pool = json.loads(pool_path.read_text())
    dims = pool["dims"]
    ceiling = pool["ceiling"]
    rng = random.Random(SEED)

    # precompute per-(dim,tier) value lists (skip empties)
    have = {(d, t): rows for d, tiers in dims.items() for t, rows in tiers.items() if rows}

    lines: list[str] = []
    skipped = 0
    attempts = 0
    max_attempts = max(N * 10, 1_000)
    while len(lines) < N:
        attempts += 1
        if attempts > max_attempts:
            raise SystemExit(
                f"unable to generate {N} requests after {max_attempts} attempts; "
                f"generated {len(lines)}, skipped {skipped}"
            )
        rpc = weighted(rng, RPC_MIX)
        dim = rng.choice(RPC_DIMS[rpc])
        # choose a tier that actually has values for this dim
        tiers_here = [t for t in TIER_MIX if (dim, t) in have]
        if not tiers_here:
            skipped += 1
            continue
        tier = weighted(rng, TIER_MIX, allowed=tiers_here)
        row = rng.choice(have[(dim, tier)])           # UNIFORM over the pool -> max spread
        v = str(row["v"])

        # random checkpoint-window start within the value's active band -> spread the
        # checkpoint-bucket suffix of the row key. end = band top; one page walks forward.
        lo = max(int(row.get("lo", 0)), FLOOR)   # clamp to target's served floor
        hi = int(row.get("hi", ceiling))
        if hi <= lo:
            # value's whole active band is below the floor -> not servable by this target
            skipped += 1
            continue
        start = lo + int(rng.random() * (hi - lo) * 0.98)

        heavy = rng.random() < HEAVY_FRAC
        read_mask = (cb.HEAVY_READ_MASK if heavy else cb.CHEAP_READ_MASK)[rpc]
        ordering = cb.ORDER_DESC if rng.random() < 0.5 else cb.ORDER_ASC

        try:
            filt = cb.f_single(PREDICATE[dim](v))
            req = cb.request(rpc, end_checkpoint=hi, start_checkpoint=start,
                             filter=filt, read_mask=read_mask,
                             opts=cb.options(limit=PAGE, ordering=ordering))
        except Exception:
            skipped += 1
            continue
        lines.append(json.dumps({"rpc": rpc, "dim": dim, "tier": tier, "request": req},
                                separators=(",", ":")))

    out = HERE / f"load.{NET}.jsonl"
    out.write_text("\n".join(lines) + "\n")
    manifest = {
        "net": NET, "seed": SEED, "n": len(lines), "skipped": skipped,
        "page": PAGE, "floor": FLOOR, "heavy_frac": HEAVY_FRAC, "rpc_mix": RPC_MIX, "tier_mix": TIER_MIX,
        "pool_generated_at": pool.get("generated_at"), "ceiling": ceiling,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    (HERE / f"load_manifest.{NET}.json").write_text(json.dumps(manifest, indent=2))
    print(f"wrote {len(lines)} requests (skipped {skipped}) -> {out}")
    print("manifest:", json.dumps(manifest))


if __name__ == "__main__":
    main()
