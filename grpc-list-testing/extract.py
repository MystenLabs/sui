# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Assemble the v0 corpus from Snowflake tiering results + exact oracle counts.

Pipeline:
  1. read queries/out/<dim>.json (tiering results, produced by tier_*.sql).
  2. pick one representative value per (dimension, tier).
  3. for each case, compute an EXACT oracle count via a targeted Snowflake query
     at the RPC's output grain (tx / event / checkpoint), cached to disk.
  4. emit corpus.jsonl (+ manifest.json) via corpus_builder.

Combinator cases (OR/NOT/unanchored) carry a `decomposition` oracle that
references their component single-literal cases by id — no SQL needed; the
correctness runner derives the expectation by set algebra (§3 of testing_plan).

All queries are frozen at CEILING (end_checkpoint exclusive) for reproducibility.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

import corpus_builder as b

# --- network parameters ---------------------------------------------------------

# CP_CEILING must be <= the kv-rpc cluster's *bitmap-index* frontier (the
# kvstore_*_dimensions watermark) -- which LAGS the raw checkpoint tip and Snowflake.
# Both the Snowflake oracle and the gRPC requests clamp to it (end_checkpoint), so it
# MUST be a checkpoint both fully serve. Override per cluster with --ceiling=<N>.
NETS = {
    # mainnet kv-rpc archival isn't backfilled yet, so the only backend is the v2alpha
    # fullnode (sui-node-mainnet-rpc-alpha), which PRUNES: it serves ~[285.6M, tip]. The
    # window must sit inside both the fullnode's retained range AND Snowflake's coverage
    # (hi ~293.13M). ceiling 293M / window 6M -> shared [287M, 293M], ~1.4M above the
    # (rising) prune floor. No archival_only cases are servable by a pruning fullnode.
    "mainnet": {"schema": "CHAINDATA_MAINNET", "ceiling": 293_000_000, "window": 6_000_000},
    # testnet shared_hi caps the exact-count window below the analytics MOVE_CALL data gap at
    # cp 342,206,316-342,208,925 (Snowflake dropped a batch; the RPC is correct -- see NOTE_ANALYTICS_GAP.md).
    "testnet": {"schema": "CHAINDATA_TESTNET", "ceiling": 344_000_000, "window": 10_000_000,
                "shared_hi": 342_000_000},
}
NET = next((a for a in sys.argv[1:] if not a.startswith("-")), "testnet")
if NET not in NETS:
    raise SystemExit(f"usage: extract.py [{'|'.join(NETS)}]")
_CFG = NETS[NET]

SCHEMA = _CFG["schema"]
_CEIL_OVERRIDE = next((int(a.split("=", 1)[1]) for a in sys.argv[1:] if a.startswith("--ceiling=")), None)
CEILING = _CEIL_OVERRIDE or _CFG["ceiling"]   # end_checkpoint, EXCLUSIVE; <= bitmap-index frontier
SHARED_LO = CEILING - _CFG["window"]      # recent window start (shared set, both backends)
SHARED_HI = _CFG.get("shared_hi", CEILING)  # shared exact-count upper bound (<= CEILING); caps below analytics gaps
# --genesis=N: archival range lower bound. Default 0 (true genesis, for full-history backends).
# For a PRUNING fullnode, set to its lowest_available_checkpoint so "archival" = the retained window.
_GEN_OVERRIDE = next((int(a.split("=", 1)[1]) for a in sys.argv[1:] if a.startswith("--genesis=")), None)
GENESIS = _GEN_OVERRIDE if _GEN_OVERRIDE is not None else 0
GENESIS_BAND = 5_000_000                  # "dense from near genesis" tolerance
CONN = "nick"
WAREHOUSE = "ANALYTICS_WH"
CHANGED = "('Mutated','Created','Deleted','Wrapped','Unwrapped')"
DIMS5 = ["sender", "move_call", "emit_module", "event_type", "affected_object"]

HERE = Path(__file__).parent
QDIR = HERE / "queries" / NET
OUT = QDIR / "out"
CACHE = QDIR / "cache"
CACHE.mkdir(parents=True, exist_ok=True)

ARCHIVAL = (GENESIS, CEILING, "archival_only")
SHARED = (SHARED_LO, SHARED_HI, "shared")


# --- snowflake helper (cached) ---------------------------------------------------


def snow_json(sql: str) -> list[dict]:
    key = hashlib.sha1(sql.encode()).hexdigest()[:16]
    cached = CACHE / f"{key}.json"
    if cached.exists():
        return json.loads(cached.read_text())
    proc = subprocess.run(
        ["snow", "sql", "-c", CONN, "--warehouse", WAREHOUSE, "-q", sql, "--format", "json"],
        capture_output=True, text=True, timeout=900,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"snow failed:\n{sql}\n---\n{proc.stdout}\n{proc.stderr}")
    rows = json.loads(proc.stdout) if proc.stdout.strip() else []
    cached.write_text(json.dumps(rows))
    return rows


def scalar(sql: str) -> int:
    rows = snow_json(sql)
    if not rows:
        return 0
    return int(next(iter(rows[0].values())) or 0)

# --- tiering SQL generation (per network) ----------------------------------------

_TIER_CFG = {
    "sender": ("TRANSACTION", "SENDER", " AND SENDER IS NOT NULL"),
    "move_call": ("MOVE_CALL", "PACKAGE || '::' || MODULE || '::' || FUNCTION_",
                  " AND PACKAGE IS NOT NULL AND MODULE IS NOT NULL AND FUNCTION_ IS NOT NULL"),
    "emit_module": ("EVENT", "PACKAGE || '::' || MODULE",
                    " AND PACKAGE IS NOT NULL AND MODULE IS NOT NULL"),
    "event_type": ("EVENT", "EVENT_TYPE", " AND EVENT_TYPE IS NOT NULL"),
}


def _generic_tier_sql(table: str, valexpr: str, notnull: str) -> str:
    return f"""WITH agg AS (
  SELECT {valexpr} v, COUNT(*) cnt, MIN(CHECKPOINT) lo, MAX(CHECKPOINT) hi
  FROM {SCHEMA}.{table}
  WHERE CHECKPOINT < {CEILING}{notnull}
  GROUP BY 1
),
tiered AS (
  SELECT v, cnt, lo, hi,
    CASE
      WHEN lo < {GENESIS_BAND} AND hi >= {CEILING - GENESIS_BAND} THEN 'dense_everywhere'
      WHEN lo >= {SHARED_LO}                                      THEN 'recent_only'
      WHEN cnt > 5000 AND (hi - lo) < 500000                      THEN 'bursty'
      WHEN cnt BETWEEN 20 AND 500                                 THEN 'sparse'
      ELSE 'other'
    END tier
  FROM agg
)
SELECT tier, v, cnt, lo, hi
FROM (SELECT tiered.*, ROW_NUMBER() OVER (PARTITION BY tier ORDER BY cnt DESC) rn FROM tiered)
WHERE tier <> 'other' AND rn <= 3
ORDER BY tier, cnt DESC;
"""


def _affected_object_tier_sql() -> str:
    return f"""WITH dense AS (
  SELECT 'dense_everywhere' tier, f.value[0]::string v, f.value[1]::int cnt
  FROM (SELECT APPROX_TOP_K(OBJECT_ID, 10, 100000) topk
        FROM {SCHEMA}.TRANSACTION_OBJECT
        WHERE CHECKPOINT < {CEILING} AND OBJECT_STATUS IN {CHANGED}),
       LATERAL FLATTEN(input => topk) f
),
recent_window AS (
  SELECT 'recent_only' tier, OBJECT_ID v, COUNT(*) cnt
  FROM {SCHEMA}.TRANSACTION_OBJECT
  WHERE CHECKPOINT BETWEEN {SHARED_LO} AND {SHARED_HI} AND OBJECT_STATUS IN {CHANGED}
  GROUP BY OBJECT_ID HAVING COUNT(*) > 200 ORDER BY COUNT(*) DESC LIMIT 3
),
sparse AS (
  SELECT 'sparse' tier, OBJECT_ID v, COUNT(*) cnt
  FROM {SCHEMA}.TRANSACTION_OBJECT
  WHERE CHECKPOINT BETWEEN {CEILING - 1_000_000} AND {CEILING} AND OBJECT_STATUS IN {CHANGED}
  GROUP BY OBJECT_ID HAVING COUNT(*) BETWEEN 20 AND 200 LIMIT 3
)
SELECT tier, v, cnt FROM dense
UNION ALL SELECT tier, v, cnt FROM recent_window
UNION ALL SELECT tier, v, cnt FROM sparse;
"""


def build_tier_sql(dim: str) -> str:
    if dim == "affected_object":
        return _affected_object_tier_sql()
    return _generic_tier_sql(*_TIER_CFG[dim])


def write_tier_queries() -> None:
    QDIR.mkdir(parents=True, exist_ok=True)
    for dim in DIMS5:
        (QDIR / f"tier_{dim}.sql").write_text(build_tier_sql(dim))


def run_tiering() -> None:
    """Run the 5 tiering queries in parallel; skip any with cached output."""
    OUT.mkdir(parents=True, exist_ok=True)
    procs = {}
    for dim in DIMS5:
        outp = OUT / f"{dim}.json"
        if outp.exists() and outp.stat().st_size > 2:
            print(f"  tiering {dim}: cached")
            continue
        fh = open(outp, "w")
        procs[dim] = (subprocess.Popen(
            ["snow", "sql", "-c", CONN, "--warehouse", WAREHOUSE,
             "-f", str(QDIR / f"tier_{dim}.sql"), "--format", "json"],
            stdout=fh, stderr=subprocess.PIPE, text=True), fh)
        print(f"  tiering {dim}: launched")
    for dim, (p, fh) in procs.items():
        _, err = p.communicate()
        fh.close()
        if p.returncode != 0:
            raise RuntimeError(f"tiering {dim} failed: {(err or '')[:300]}")
        print(f"  tiering {dim}: done")


# --- tiering result loading ------------------------------------------------------


def load_tiers(dim: str) -> dict[str, list[dict]]:
    """Return {tier: [rows]} sorted by cnt desc within each tier."""
    path = OUT / f"{dim}.json"
    rows = json.loads(path.read_text())
    out: dict[str, list[dict]] = {}
    for r in rows:
        r = {k.lower(): v for k, v in r.items()}
        out.setdefault(r["tier"], []).append(r)
    for tier in out:
        out[tier].sort(key=lambda r: r.get("cnt") or 0, reverse=True)
    return out


def pick(tiers: dict[str, list[dict]], tier: str, i: int = 0) -> dict | None:
    rows = tiers.get(tier) or []
    return rows[i] if i < len(rows) else None


# --- oracle counts (grain depends on RPC) ----------------------------------------

# value-matching SQL fragment per (dimension, specificity), parameterized on the
# literal value string. Returns a WHERE predicate over the dimension's table.
def _match(dim: str, spec: str, v: str) -> tuple[str, str, str]:
    """(table, where_predicate, extra_filter) for a value at a specificity."""
    q = v.replace("'", "''")
    if dim == "sender":
        return "TRANSACTION", f"SENDER = '{q}'", ""
    if dim == "affected_object":
        return "TRANSACTION_OBJECT", f"OBJECT_ID = '{q}'", f"AND OBJECT_STATUS IN {CHANGED}"
    if dim == "move_call":
        if spec == "package":
            return "MOVE_CALL", f"PACKAGE = '{q}'", ""
        if spec == "module":
            return "MOVE_CALL", f"PACKAGE || '::' || MODULE = '{q}'", ""
        return "MOVE_CALL", f"PACKAGE || '::' || MODULE || '::' || FUNCTION_ = '{q}'", ""
    if dim == "emit_module":
        if spec == "package":
            return "EVENT", f"PACKAGE = '{q}'", ""
        return "EVENT", f"PACKAGE || '::' || MODULE = '{q}'", ""
    if dim == "event_type":
        # STARTSWITH (not LIKE '%') — snow's templating treats `<%` as a tag and
        # `%'` trips its tokenizer; STARTSWITH avoids both.
        if spec in ("address", "module"):
            return "EVENT", f"STARTSWITH(EVENT_TYPE, '{q}::')", ""
        if spec == "name":  # any instantiation
            return "EVENT", f"(EVENT_TYPE = '{q}' OR STARTSWITH(EVENT_TYPE, '{q}<'))", ""
        return "EVENT", f"EVENT_TYPE = '{q}'", ""
    raise ValueError(dim)


_GRAIN_EXPR = {
    "tx": "COUNT(DISTINCT TRANSACTION_DIGEST)",
    "event": "COUNT(*)",
    "checkpoint": "COUNT(DISTINCT CHECKPOINT)",
}


def oracle_count(dim: str, spec: str, v: str, lo: int, hi: int, grain: str) -> int:
    table, where, extra = _match(dim, spec, v)
    if dim == "sender" and grain == "event":
        # ListEvents(sender=S) counts EVENTS emitted by S's txns; EVENT.SENDER is the
        # emitting txn's sender. Counting over TRANSACTION yields the txn count (wrong grain).
        table, extra = "EVENT", ""
    # sender on TRANSACTION: one row per tx, so tx-grain is COUNT(*).
    expr = "COUNT(*)" if (grain == "tx" and table == "TRANSACTION") else _GRAIN_EXPR[grain]
    sql = (
        f"SELECT {expr} c FROM {SCHEMA}.{table} "
        f"WHERE {where} {extra} AND CHECKPOINT >= {lo} AND CHECKPOINT < {hi}"
    )
    return scalar(sql)


def grain_for(rpc: str) -> str:
    return {"ListTransactions": "tx", "ListEvents": "event", "ListCheckpoints": "checkpoint"}[rpc]


# --- case assembly helpers -------------------------------------------------------

RPC_PREFIX = {"ListTransactions": "tx", "ListEvents": "ev", "ListCheckpoints": "cp"}


def predicate(dim: str, spec: str, v: str):
    if dim == "sender":
        return b.sender(v)
    if dim == "affected_address":
        return b.affected_address(v)
    if dim == "affected_object":
        return b.affected_object(v)
    if dim == "move_call":
        return b.move_call(v)
    if dim == "emit_module":
        return b.emit_module(v)
    if dim == "event_type":
        return b.event_type(v)
    if dim == "event_stream_head":
        return b.event_stream_head(v)
    if dim == "package_write":
        return b.package_write()
    raise ValueError(dim)


def make_single(cases, reg, *, rpc, dim, spec, v, lo, hi, scope, tier, cost,
                ordering=b.ORDER_ASC, limit_items=1000, read_mask="__auto__", id_extra=""):
    vh = hashlib.sha1(v.encode()).hexdigest()[:6]
    osfx = "asc" if ordering == b.ORDER_ASC else "desc"
    cid = f"{RPC_PREFIX[rpc]}.{dim}.{spec}.{tier}.{scope}.{cost}.{osfx}{id_extra}.{vh}"
    if ordering == b.ORDER_ASC:
        reg[(rpc, dim, v, scope)] = cid
    existing = next((c for c in cases if c.id == cid), None)
    if existing is not None:  # idempotent
        return cid, existing.oracle.expected_count
    grain = grain_for(rpc)
    try:
        cnt, kind = oracle_count(dim, spec, v, lo, hi, grain), "exact_count"
    except Exception as e:  # one bad oracle must not abort the whole corpus
        print(f"  WARN oracle failed for {cid}: {str(e).splitlines()[-1][:120]}", file=sys.stderr)
        cnt, kind = None, "oracle_failed"
    rm = (b.HEAVY_READ_MASK[rpc] if cost == "expensive" else None) if read_mask == "__auto__" else read_mask
    cases.append(b.Case(
        id=cid, rpc=rpc,
        request=b.request(rpc, start_checkpoint=lo, end_checkpoint=hi,
                          filter=b.f_single(predicate(dim, spec, v)),
                          read_mask=rm, opts=b.options(limit_items=limit_items, ordering=ordering)),
        klass=b.Klass(dim, "single", tier, cost, scope, specificity=spec),
        oracle=b.Oracle(kind, expected_count=cnt, sql_ref=f"extract.py:_match({dim},{spec})"),
    ))
    return cid, cnt


def specificity_levels(dim: str, v: str) -> list[tuple[str, str]]:
    """[(specificity, value)] from a full value, coarse->fine."""
    if dim == "move_call":
        p = v.split("::")
        return [("package", p[0]), ("module", "::".join(p[:2])), ("function", v)]
    if dim == "emit_module":
        p = v.split("::")
        return [("package", p[0]), ("module", v)]
    if dim == "event_type":
        head = v.split("<", 1)[0]
        p = head.split("::")
        out = [("address", p[0])]
        if len(p) >= 2:
            out.append(("module", "::".join(p[:2])))
        if len(p) >= 3:
            out.append(("name", head))
        out.append(("full", v))
        return out
    return [("na", v)]

# --- AND oracle via SQL join (sender x move_call) --------------------------------


def and_count(sender_v: str, mc_v: str, lo: int, hi: int, grain: str) -> int:
    s = sender_v.replace("'", "''")
    m = mc_v.replace("'", "''")
    expr = "COUNT(DISTINCT t.CHECKPOINT)" if grain == "checkpoint" else "COUNT(DISTINCT t.TRANSACTION_DIGEST)"
    sql = (
        f"SELECT {expr} c FROM {SCHEMA}.TRANSACTION t "
        f"JOIN {SCHEMA}.MOVE_CALL m ON t.TRANSACTION_DIGEST = m.TRANSACTION_DIGEST "
        f"WHERE t.SENDER = '{s}' "
        f"AND m.PACKAGE || '::' || m.MODULE || '::' || m.FUNCTION_ = '{m}' "
        f"AND t.CHECKPOINT >= {lo} AND t.CHECKPOINT < {hi} "
        f"AND m.CHECKPOINT >= {lo} AND m.CHECKPOINT < {hi}"
    )
    return scalar(sql)


def unfiltered_count(rpc: str, lo: int, hi: int) -> int:
    grain = grain_for(rpc)
    if grain == "checkpoint":
        return hi - lo  # every checkpoint in [lo, hi) exists
    table = "EVENT" if grain == "event" else "TRANSACTION"
    return scalar(f"SELECT COUNT(*) c FROM {SCHEMA}.{table} WHERE CHECKPOINT >= {lo} AND CHECKPOINT < {hi}")


def package_write_count(rpc: str, lo: int, hi: int) -> int:
    grain = grain_for(rpc)
    expr = "COUNT(DISTINCT CHECKPOINT)" if grain == "checkpoint" else "COUNT(DISTINCT TRANSACTION_DIGEST)"
    return scalar(f"SELECT {expr} c FROM {SCHEMA}.MOVE_PACKAGE WHERE CHECKPOINT >= {lo} AND CHECKPOINT < {hi}")


def discover_sender_movecall(lo: int, hi: int):
    """A real co-occurring (sender, move_call) pair -> (sender, path, tx_count)."""
    sql = (f"SELECT t.SENDER s, m.PACKAGE || '::' || m.MODULE || '::' || m.FUNCTION_ p, "
           f"COUNT(DISTINCT t.TRANSACTION_DIGEST) c FROM {SCHEMA}.TRANSACTION t "
           f"JOIN {SCHEMA}.MOVE_CALL m ON t.TRANSACTION_DIGEST = m.TRANSACTION_DIGEST "
           f"WHERE t.CHECKPOINT >= {lo} AND t.CHECKPOINT < {hi} "
           f"AND m.CHECKPOINT >= {lo} AND m.CHECKPOINT < {hi} "
           f"GROUP BY 1, 2 HAVING c BETWEEN 100 AND 100000 ORDER BY c DESC LIMIT 1")
    rows = snow_json(sql)
    if not rows:
        return None
    r = {k.lower(): v for k, v in rows[0].items()}
    return r["s"], r["p"], int(r["c"])


def discover_sender_emitmodule(lo: int, hi: int):
    """A real co-occurring (sender, emit_module) pair in event-space -> (sender, module, event_count)."""
    sql = (f"SELECT SENDER s, PACKAGE || '::' || MODULE em, COUNT(*) c FROM {SCHEMA}.EVENT "
           f"WHERE CHECKPOINT >= {lo} AND CHECKPOINT < {hi} AND MODULE IS NOT NULL "
           f"GROUP BY 1, 2 HAVING c BETWEEN 100 AND 100000 ORDER BY c DESC LIMIT 1")
    rows = snow_json(sql)
    if not rows:
        return None
    r = {k.lower(): v for k, v in rows[0].items()}
    return r["s"], r["em"], int(r["c"])


COMPOUND = {"move_call", "emit_module", "event_type"}


def main() -> None:
    print(f"network={NET} schema={SCHEMA} ceiling={CEILING} shared_lo={SHARED_LO}")
    write_tier_queries()
    run_tiering()
    T = {d: load_tiers(d) for d in DIMS5}
    cases: list[b.Case] = []
    reg: dict = {}

    def add_case(cid, rpc, request, klass, oracle):
        if not any(c.id == cid for c in cases):
            cases.append(b.Case(id=cid, rpc=rpc, request=request, klass=klass, oracle=oracle))

    def ref(rpc, dim, v):
        return reg.get((rpc, dim, v, "shared"))

    def safe(fn, *a):
        try:
            return fn(*a)
        except Exception as e:
            print(f"  WARN query failed: {str(e).splitlines()[-1][:120]}", file=sys.stderr)
            return None

    def spec_of(dim, v):
        if dim == "event_type":
            return "generic" if "<" in v else "full"
        return "full" if dim in COMPOUND else "na"

    def ranges_for(tier, rpc):
        if tier == "dense_everywhere":
            r = [(ARCHIVAL, "expensive"), (SHARED, "expensive")]
            if rpc == "ListTransactions":
                r.append((SHARED, "cheap"))
            return r
        if tier == "recent_only":
            return [(SHARED, "cheap")]
        if tier in ("sparse", "bursty"):
            return [(ARCHIVAL, "cheap")]
        return []

    TX_DIMS = ["sender", "move_call", "emit_module", "event_type", "affected_object"]
    EV_DIMS = ["sender", "emit_module", "event_type"]
    RPC_DIMS = {"ListTransactions": TX_DIMS, "ListCheckpoints": TX_DIMS, "ListEvents": EV_DIMS}

    # ---- A. systematic singles: rpc x dim x tier (+ desc twin on shared) ----
    for rpc, ds in RPC_DIMS.items():
        for dim in ds:
            for tier in ("dense_everywhere", "recent_only", "sparse", "bursty"):
                row = pick(T[dim], tier)
                if not row:
                    continue
                spec = spec_of(dim, row["v"])
                for (lo, hi, scope), cost in ranges_for(tier, rpc):
                    make_single(cases, reg, rpc=rpc, dim=dim, spec=spec, v=row["v"],
                                lo=lo, hi=hi, scope=scope, tier=tier, cost=cost)
                    if scope == "shared":  # asc == reverse(desc) twin (free oracle)
                        make_single(cases, reg, rpc=rpc, dim=dim, spec=spec, v=row["v"],
                                    lo=lo, hi=hi, scope=scope, tier=tier, cost=cost,
                                    ordering=b.ORDER_DESC)

    # ---- package_write + unfiltered (universe primitives) ----
    for rpc in ("ListTransactions", "ListCheckpoints"):
        for (lo, hi, scope) in (ARCHIVAL, SHARED):
            cnt = safe(package_write_count, rpc, lo, hi)
            add_case(f"{RPC_PREFIX[rpc]}.package_write.na.dense_everywhere.{scope}", rpc,
                b.request(rpc, start_checkpoint=lo, end_checkpoint=hi,
                          filter=b.f_single(b.package_write()), opts=b.options(limit_items=1000)),
                b.Klass("package_write", "single", "dense_everywhere", "expensive", scope, specificity="na"),
                b.Oracle("exact_count", expected_count=cnt, sql_ref="extract.py:package_write_count"))
    for rpc in b.RPCS:
        for (lo, hi, scope) in (ARCHIVAL, SHARED):
            cnt = safe(unfiltered_count, rpc, lo, hi)
            cid = f"{RPC_PREFIX[rpc]}.unfiltered.{scope}"
            add_case(cid, rpc,
                b.request(rpc, start_checkpoint=lo, end_checkpoint=hi, opts=b.options(limit_items=1000)),
                b.Klass("unfiltered", "single", "na", "cheap", scope, specificity="na"),
                b.Oracle("exact_count", expected_count=cnt, sql_ref="extract.py:unfiltered_count"))
            reg[("unfiltered", rpc, scope)] = cid

    # ---- B. specificity ladders (compound dims, dense, archival) ----
    spec_rpc = {"ListTransactions": COMPOUND, "ListCheckpoints": COMPOUND,
                "ListEvents": {"emit_module", "event_type"}}
    for rpc, dimset in spec_rpc.items():
        for dim in dimset:
            d = pick(T[dim], "dense_everywhere")
            if not d:
                continue
            for spec, val in specificity_levels(dim, d["v"]):
                if spec == "full":
                    continue
                make_single(cases, reg, rpc=rpc, dim=dim, spec=spec, v=val,
                            lo=ARCHIVAL[0], hi=ARCHIVAL[1], scope="archival_only",
                            tier="dense_everywhere", cost="expensive")

    # ---- D. read_mask matrix (ListTransactions, dense-shared) ----
    for dim in ("sender", "move_call"):
        d = pick(T[dim], "dense_everywhere")
        if not d:
            continue
        sp = spec_of(dim, d["v"])
        for label, rm, cost in (("digest", "transaction.digest", "cheap"),
                                ("heavy", "transaction", "expensive")):
            make_single(cases, reg, rpc="ListTransactions", dim=dim, spec=sp, v=d["v"],
                        lo=SHARED[0], hi=SHARED[1], scope="shared", tier="dense_everywhere",
                        cost=cost, read_mask=rm, id_extra=f".rm_{label}")

    # ---- E. limit + range-edge ----
    sd = pick(T["sender"], "dense_everywhere", 1) or pick(T["sender"], "dense_everywhere")
    if sd:
        sv = sd["v"]
        tot = safe(oracle_count, "sender", "na", sv, SHARED[0], SHARED[1], "tx")
        add_case("tx.sender.edge_limit5.dense.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_single(b.sender(sv)), opts=b.options(limit_items=5)),
            b.Klass("sender", "single", "dense_everywhere", "cheap", "shared", specificity="na"),
            b.Oracle("exact_count", expected_count=tot, expected_end_reason="QUERY_END_REASON_ITEM_LIMIT",
                     sql_ref="returns min(5,total) then ITEM_LIMIT"))
        add_case("tx.sender.edge_overcap.dense.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_single(b.sender(sv)), opts=b.options(limit_items=100_000_000)),
            b.Klass("sender", "single", "dense_everywhere", "cheap", "shared", specificity="na"),
            b.Oracle("exact_count", expected_count=tot, sql_ref="server coerces limit to max"))
        ge = safe(oracle_count, "sender", "na", sv, 0, 1_000_000, "tx")
        add_case("tx.sender.edge_genesis.dense", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=0, end_checkpoint=1_000_000,
                      filter=b.f_single(b.sender(sv)), opts=b.options(limit_items=1000)),
            b.Klass("sender", "single", "dense_everywhere", "cheap", "archival_only", specificity="na"),
            b.Oracle("exact_count", expected_count=ge, sql_ref="extract.py:_match(sender,na)"))
        te = safe(oracle_count, "sender", "na", sv, CEILING - 100_000, CEILING, "tx")
        add_case("tx.sender.edge_tip.dense", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=CEILING - 100_000, end_checkpoint=CEILING,
                      filter=b.f_single(b.sender(sv)), opts=b.options(limit_items=1000)),
            b.Klass("sender", "single", "dense_everywhere", "cheap", "shared", specificity="na"),
            b.Oracle("exact_count", expected_count=te, sql_ref="extract.py:_match(sender,na)"))
    srr = pick(T["sender"], "recent_only")
    if srr:
        ec = safe(oracle_count, "sender", "na", srr["v"], 0, 1_000_000, "tx")
        add_case("tx.sender.edge_empty_range.recent_over_genesis", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=0, end_checkpoint=1_000_000,
                      filter=b.f_single(b.sender(srr["v"])), opts=b.options(limit_items=1000)),
            b.Klass("sender", "single", "recent_only", "cheap", "archival_only", specificity="na"),
            b.Oracle("exact_count", expected_count=ec, sql_ref="recent value over genesis -> expect ~0"))

    # ---- F. combinators (ListTransactions) ----
    sa, sb = pick(T["sender"], "recent_only", 0), pick(T["sender"], "recent_only", 1)
    if sa and sb and ref("ListTransactions", "sender", sa["v"]) and ref("ListTransactions", "sender", sb["v"]):
        add_case("tx.sender.or_samedim.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_or(b.sender(sa["v"]), b.sender(sb["v"])), opts=b.options(limit_items=1000)),
            b.Klass("sender", "or", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="union",
                     components=(ref("ListTransactions", "sender", sa["v"]), ref("ListTransactions", "sender", sb["v"]))))
    mr = pick(T["move_call"], "recent_only")
    if sa and mr and ref("ListTransactions", "sender", sa["v"]) and ref("ListTransactions", "move_call", mr["v"]):
        add_case("tx.sender_or_move_call.crossdim.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_or(b.sender(sa["v"]), b.move_call(mr["v"])), opts=b.options(limit_items=1000)),
            b.Klass("sender+move_call", "or", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="union",
                     components=(ref("ListTransactions", "sender", sa["v"]), ref("ListTransactions", "move_call", mr["v"]))))
    # deep-DNF: OR of up to 4 senders (fanout / max_literals stress)
    quad = [q for q in (pick(T["sender"], "recent_only", i) for i in range(3)) if q]
    extra = pick(T["sender"], "dense_everywhere", 1)
    if extra:
        quad.append(extra)
    comps = []
    for q in quad:
        idq = ref("ListTransactions", "sender", q["v"])
        if not idq:
            idq, _ = make_single(cases, reg, rpc="ListTransactions", dim="sender", spec="na", v=q["v"],
                                 lo=SHARED[0], hi=SHARED[1], scope="shared", tier="dense_everywhere", cost="cheap")
        comps.append(idq)
    if len(comps) >= 3:
        add_case("tx.sender.or_deep_dnf.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_or(*[b.sender(q["v"]) for q in quad]), opts=b.options(limit_items=1000)),
            b.Klass("sender", "or", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="union", components=tuple(comps)))
    # AND overlap (nonzero) + anchored NOT sharing the pair
    pair = safe(discover_sender_movecall, SHARED[0], SHARED[1])
    if pair:
        ps, pm, pc = pair
        id_ps, _ = make_single(cases, reg, rpc="ListTransactions", dim="sender", spec="na", v=ps,
                               lo=SHARED[0], hi=SHARED[1], scope="shared", tier="mixed", cost="expensive")
        id_and = "tx.sender_and_move_call.overlap.shared"
        add_case(id_and, "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_and(b.sender(ps), b.move_call(pm)), opts=b.options(limit_items=1000)),
            b.Klass("sender+move_call", "and", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("exact_count", expected_count=pc, sql_ref="extract.py:discover_sender_movecall"))
        add_case("tx.sender_not_move_call.anchored.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_and_not(b.sender(ps), b.move_call(pm)), opts=b.options(limit_items=1000)),
            b.Klass("sender+move_call", "not", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="difference", components=(id_ps, id_and)))
    # unanchored NOT (tx): NOT recent sender
    uni_tx = reg.get(("unfiltered", "ListTransactions", "shared"))
    if sa and ref("ListTransactions", "sender", sa["v"]) and uni_tx:
        add_case("tx.sender.unanchored_not.recent.shared", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_single(b.sender(sa["v"]), negate=True), opts=b.options(limit_items=1000)),
            b.Klass("sender", "not", "recent_only", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="difference",
                     components=(uni_tx, ref("ListTransactions", "sender", sa["v"]))))

    # ---- event-space combinators (ListEvents) ----
    emd = pick(T["emit_module"], "dense_everywhere")
    etd = pick(T["event_type"], "dense_everywhere")
    uni_ev = reg.get(("unfiltered", "ListEvents", "shared"))
    if emd and etd and ref("ListEvents", "emit_module", emd["v"]) and ref("ListEvents", "event_type", etd["v"]):
        add_case("ev.emit_or_eventtype.crossdim.shared", "ListEvents",
            b.request("ListEvents", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_or(b.emit_module(emd["v"]), b.event_type(etd["v"])), opts=b.options(limit_items=1000)),
            b.Klass("emit_module+event_type", "or", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="union",
                     components=(ref("ListEvents", "emit_module", emd["v"]), ref("ListEvents", "event_type", etd["v"]))))
    if emd and ref("ListEvents", "emit_module", emd["v"]) and uni_ev:
        add_case("ev.emit_module.unanchored_not.dense.shared", "ListEvents",
            b.request("ListEvents", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_single(b.emit_module(emd["v"]), negate=True), opts=b.options(limit_items=1000)),
            b.Klass("emit_module", "not", "dense_everywhere", "expensive", "shared", specificity="na"),
            b.Oracle("decomposition", relation="difference",
                     components=(uni_ev, ref("ListEvents", "emit_module", emd["v"]))))
    epair = safe(discover_sender_emitmodule, SHARED[0], SHARED[1])
    if epair:
        es, eem, eec = epair
        add_case("ev.sender_and_emit_module.overlap.shared", "ListEvents",
            b.request("ListEvents", start_checkpoint=SHARED[0], end_checkpoint=SHARED[1],
                      filter=b.f_and(b.sender(es), b.emit_module(eem)), opts=b.options(limit_items=1000)),
            b.Klass("sender+emit_module", "and", "mixed", "expensive", "shared", specificity="na"),
            b.Oracle("exact_count", expected_count=eec, sql_ref="extract.py:discover_sender_emitmodule"))

    # ---- G. event_stream_head synthetic valid-but-empty (unlaunched, SS4.4) ----
    sh = "0x" + "5ea4" * 16
    for rpc in ("ListEvents", "ListTransactions"):
        add_case(f"{RPC_PREFIX[rpc]}.event_stream_head.synthetic_empty.archival", rpc,
            b.request(rpc, start_checkpoint=ARCHIVAL[0], end_checkpoint=ARCHIVAL[1],
                      filter=b.f_single(b.event_stream_head(sh)),
                      read_mask=("transaction.digest" if rpc == "ListTransactions" else None),
                      opts=b.options(limit_items=1000)),
            b.Klass("event_stream_head", "single", "empty_degenerate", "adversarial", "archival_only", specificity="na"),
            b.Oracle("degenerate", expected_count=0, expected_end_reason="QUERY_END_REASON_CHECKPOINT_BOUND",
                     sql_ref="synthetic: event_stream_head unlaunched (testing_plan SS4.4)"))

    # ---- H. degenerate (abuse Q2): system sender AND dense move_call -> ~0 ----
    sys_sender = "0x" + "0" * 64
    mc_dense = pick(T["move_call"], "dense_everywhere")
    if mc_dense:
        deg = safe(and_count, sys_sender, mc_dense["v"], SHARED[0], SHARED[1], "tx")
        add_case("tx.degenerate.dense_and_dense.empty.archival", "ListTransactions",
            b.request("ListTransactions", start_checkpoint=ARCHIVAL[0], end_checkpoint=ARCHIVAL[1],
                      filter=b.f_and(b.sender(sys_sender), b.move_call(mc_dense["v"])),
                      read_mask="transaction.digest", opts=b.options(limit_items=1000)),
            b.Klass("sender+move_call", "and", "empty_degenerate", "adversarial", "archival_only", specificity="na"),
            b.Oracle("degenerate", expected_count=deg, expected_end_reason="QUERY_END_REASON_SCAN_LIMIT",
                     sql_ref="extract.py:and_count"))

    # ---- emit ----
    out = HERE / f"corpus.{NET}.jsonl"
    n = b.write_corpus(cases, str(out))
    manifest = {
        "network": NET,
        "cp_ceiling": CEILING, "shared_window_lo": SHARED_LO, "shared_window_hi": SHARED_HI, "genesis": GENESIS,
        "schema": SCHEMA, "warehouse": WAREHOUSE, "connection": CONN,
        "proto_rev": "43c5bc13202ae398b1519a3eead1f40df8ca277b",
        "n_cases": n, "ordering_default": "ORDERING_ASCENDING",
        "notes": "end_checkpoint exclusive; Snowflake oracle uses CHECKPOINT < cp_ceiling. "
                 "cp_ceiling MUST be <= target kv-rpc cluster backfilled tip.",
    }
    (HERE / f"manifest.{NET}.json").write_text(json.dumps(manifest, indent=2))
    # summary
    import collections as _c
    def tally(key):
        return dict(_c.Counter(key(x) for x in cases))
    print(f"wrote {n} cases -> {out}")
    print("by rpc:", tally(lambda c: c.rpc))
    print("by combinator:", tally(lambda c: c.klass.combinator))
    print("by tier:", tally(lambda c: c.klass.selectivity_tier))
    print("by scope:", tally(lambda c: c.klass.backend_scope))
    print("by ordering:", tally(lambda c: c.request.get("options", {}).get("ordering", "ASC")))
    print("by oracle:", tally(lambda c: c.oracle.kind))


if __name__ == "__main__":
    main()
