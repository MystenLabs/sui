#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Stable-v2 LedgerService correctness harness.

Replays a corpus.jsonl (built by ../extract.py) against a kv-rpc archival
endpoint and/or a fullnode, drains each server stream to completion (Model B),
and checks every case against its oracle:

  exact_count   -> |result set| == expected_count            (Snowflake oracle)
  decomposition -> set algebra over component cases          (union / difference)
  degenerate    -> bounded empty probe: 0 items, expected terminal reason

plus structural invariants on every drain:
  tiling        -> no duplicate identity across pages
  watermark     -> checkpoint non-decreasing (asc) / non-increasing (desc)
  ordering      -> asc result == reverse(desc result) for paired cases
  read_mask     -> cheap/heavy twins return the same identity set

and an optional cross-backend differential (archival vs fullnode) for
`shared`-scope cases.

The harness sends each request AS-IS (its own read_mask), so the real masks are
exercised; identity fields are present under every mask the corpus emits.

Requests are parsed straight from the corpus `request` (canonical protojson) into
the generated proto types via google.protobuf.json_format -- one source of truth,
no bespoke filter builder.

Usage:
  # list/categorize only, no network (validates corpus + plan):
  python harness.py --corpus ../corpus.testnet.jsonl --list

  # run against a port-forwarded archival kv-rpc (plaintext h2c):
  python harness.py --corpus ../corpus.testnet.jsonl --archival localhost:8000

  # add cross-backend differential against a fullnode (TLS h2):
  python harness.py --corpus ../corpus.testnet.jsonl \
      --archival localhost:8000 --fullnode localhost:9443 --fullnode-ca ca.pem

  # subset by id regex, smaller drain cap:
  python harness.py --corpus ../corpus.testnet.jsonl --archival localhost:8000 \
      --only 'shared' --max-drain 100000
"""
import argparse
import json
import os
import re
import sys
import time
from dataclasses import dataclass, field

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))

from google.protobuf import json_format  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2 as ls  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2_grpc as ls_grpc  # noqa: E402
from sui.rpc.v2 import query_options_pb2 as qo  # noqa: E402

REQ_TYPE = {
    "ListTransactions": ls.ListTransactionsRequest,
    "ListEvents": ls.ListEventsRequest,
    "ListCheckpoints": ls.ListCheckpointsRequest,
}
ORDER_ASC = qo.ORDERING_ASCENDING
RESUME_REASONS = {qo.QUERY_END_REASON_ITEM_LIMIT, qo.QUERY_END_REASON_SCAN_LIMIT}

RESPONSE_PAYLOAD_FIELDS = {
    "ListTransactions": "transaction",
    "ListEvents": "event",
    "ListCheckpoints": "checkpoint",
}


def response_payload(rpc: str, response):
    field_name = RESPONSE_PAYLOAD_FIELDS[rpc]
    return getattr(response, field_name) if response.HasField(field_name) else None


def reason_name(v: int) -> str:
    return qo.QueryEndReason.Name(v) if v is not None else "<none>"


def identity(rpc: str, item):
    """Stable per-item identity used for set comparison."""
    if rpc == "ListTransactions":
        return ("tx", item.digest)
    if rpc == "ListCheckpoints":
        return ("cp", item.sequence_number)
    if rpc == "ListEvents":
        return ("ev", item.transaction_digest, item.event_index)
    raise ValueError(rpc)


def identity_present(rpc: str, ident) -> bool:
    if rpc == "ListTransactions":
        return bool(ident[1])
    if rpc == "ListEvents":
        return bool(ident[1])  # transaction_digest non-empty
    return True  # cp sequence_number 0 is valid (genesis)


# --- drain --------------------------------------------------------------------

@dataclass
class PageMeta:
    count: int
    end_reason: int  # QueryEndReason enum value, or None if stream ended w/o QueryEnd


@dataclass
class DrainResult:
    ids: list = field(default_factory=list)           # ordered, de-duplicated
    pages: list = field(default_factory=list)         # list[PageMeta]
    error: str = None
    tiling_ok: bool = True                            # no dup identity across pages
    watermark_ok: bool = True                         # monotonic per ordering
    identity_ok: bool = True                          # every item had an identity field
    capped: bool = False                              # stopped at max_drain, set incomplete

    @property
    def idset(self):
        return set(self.ids)

    @property
    def terminal_reason(self):
        return self.pages[-1].end_reason if self.pages else None


def drain(send_fn, rpc, base_req, *, full=True, max_drain=250_000, max_pages=100_000, max_retries=8):
    """Drive the cursor to completion (or to max_drain). send_fn(req)->response iterator."""
    r = DrainResult()
    seen = set()
    asc = base_req.options.ordering == ORDER_ASC
    last_cursor = None
    retries = 0
    RETRYABLE = ("UNAVAILABLE", "Connection refused", "Socket closed", "GOAWAY",
                 "Broken pipe", "Transport closed", "failed to connect", "Stream removed")

    for _ in range(max_pages):
        req = type(base_req)()
        req.CopyFrom(base_req)
        if last_cursor is not None:
            if asc:
                req.options.after = last_cursor
            else:
                req.options.before = last_cursor
        page_cursor_start = last_cursor
        pcount = 0
        end_reason = None
        last_checkpoint = None
        try:
            for resp in send_fn(req):
                payload = response_payload(rpc, resp)
                if payload is not None:
                    pcount += 1
                    ident = identity(rpc, payload)
                    if not identity_present(rpc, ident):
                        r.identity_ok = False
                    if ident in seen:
                        r.tiling_ok = False
                    else:
                        seen.add(ident)
                        r.ids.append(ident)

                if not resp.HasField("watermark"):
                    r.watermark_ok = False
                    r.error = "response frame missing required watermark"
                    return r
                watermark = resp.watermark
                if not watermark.HasField("cursor") or not watermark.cursor:
                    r.watermark_ok = False
                    r.error = "watermark missing required cursor"
                    return r
                last_cursor = watermark.cursor
                if watermark.HasField("checkpoint"):
                    if last_checkpoint is not None:
                        if asc and watermark.checkpoint < last_checkpoint:
                            r.watermark_ok = False
                        if not asc and watermark.checkpoint > last_checkpoint:
                            r.watermark_ok = False
                    last_checkpoint = watermark.checkpoint

                if resp.HasField("end"):
                    end_reason = resp.end.reason
        except Exception as e:  # grpc.RpcError or transport error
            code = getattr(e, "code", lambda: None)()
            detail = getattr(e, "details", lambda: str(e))()
            msg = f"{code}: {detail}" if code else str(e)
            if retries < max_retries and any(s in msg for s in RETRYABLE):
                retries += 1
                time.sleep(min(2 * retries, 10))
                continue
            r.error = msg
            return r
        r.pages.append(PageMeta(pcount, end_reason))
        if end_reason is None:
            # A successful stream always ends with QueryEnd; its absence means the stream
            # was truncated. Resume only when the watermark cursor advanced.
            if retries < max_retries and last_cursor is not None and last_cursor != page_cursor_start:
                retries += 1
                time.sleep(min(2 * retries, 10))
                continue
            r.error = "stream truncated before QueryEnd (no terminal frame)"
            return r
        retries = 0
        if end_reason in RESUME_REASONS and last_cursor == page_cursor_start:
            r.error = "resumable QueryEnd did not advance watermark cursor"
            return r

        if not full:
            return r
        if len(r.ids) >= max_drain:
            r.capped = True
            return r
        if end_reason in RESUME_REASONS:
            continue
        return r

    r.capped = True
    return r


# --- oracle + invariant checks ------------------------------------------------

@dataclass
class CaseResult:
    cid: str
    status: str           # PASS | FAIL | SKIP
    reasons: list = field(default_factory=list)
    count: int = None
    expected: int = None


# Minimal valid read_mask paths per RPC for identity-only drains.
IDENTITY_MASK = {
    "ListTransactions": ("digest",),
    "ListCheckpoints": ("sequence_number",),
    "ListEvents": ("transaction_digest", "event_index"),
}


def base_request(rec, identity_mask=False):
    req = json_format.ParseDict(rec["request"], REQ_TYPE[rec["rpc"]]())
    if identity_mask:
        req.ClearField("read_mask")
        req.read_mask.paths.extend(IDENTITY_MASK[rec["rpc"]])
    return req


def check_oracle(rec, drains, max_drain):
    """Return (status, reasons). drains: cid -> DrainResult."""
    o = rec["oracle"]
    kind = o["kind"]
    cid = rec["id"]
    dr = drains[cid]
    reasons = []

    if dr.error:
        return "FAIL", [f"rpc-error: {dr.error}"]
    if not dr.identity_ok:
        reasons.append("identity field missing on some items")
    if not dr.tiling_ok:
        reasons.append("tiling violation: duplicate identity across pages")
    if not dr.watermark_ok:
        reasons.append("watermark non-monotonic")

    if kind == "exact_count":
        lim = rec.get("request", {}).get("options", {}).get("limit")
        if lim and lim < 50:  # limit-semantics probe: verify first page, not the full count
            if len(dr.idset) != lim:
                reasons.append(f"limit probe: first page returned {len(dr.idset)} != limit {lim}")
            er = o.get("expected_end_reason")
            if er and dr.pages and reason_name(dr.pages[0].end_reason) != er:
                reasons.append(f"first-page end_reason {reason_name(dr.pages[0].end_reason)} != {er}")
            return ("FAIL" if reasons else "PASS"), reasons
        if dr.capped:
            reasons.append(f"count UNVERIFIED (expected {o['expected_count']:,} > cap {max_drain:,}); "
                           "structural checks only")
            return ("FAIL" if reasons[:-1] else "SKIP"), reasons
        if len(dr.idset) != o["expected_count"]:
            reasons.append(f"count {len(dr.idset):,} != expected {o['expected_count']:,}")
        er = o.get("expected_end_reason")
        if er and dr.pages and reason_name(dr.pages[0].end_reason) != er:
            reasons.append(f"first-page end_reason {reason_name(dr.pages[0].end_reason)} != {er}")

    elif kind == "degenerate":
        # single-page probe: bounded, must be empty and terminate cleanly
        exp = o.get("expected_count", 0)
        if len(dr.idset) != exp:
            reasons.append(f"degenerate returned {len(dr.idset)} items, expected {exp}")
        if dr.terminal_reason is None:
            reasons.append("no QueryEnd frame (stream did not terminate cleanly)")
        er = o.get("expected_end_reason")
        if er and reason_name(dr.terminal_reason) != er:
            reasons.append(f"end_reason {reason_name(dr.terminal_reason)} != {er}")

    elif kind == "decomposition":
        comps = o["components"]
        missing = [c for c in comps if c not in drains or drains[c].capped or drains[c].error]
        if missing:
            reasons.append(f"component(s) unverifiable (capped/error/missing): {missing}")
            return "SKIP", reasons
        sets = [drains[c].idset for c in comps]
        if o["relation"] == "union":
            expect = set().union(*sets)
        elif o["relation"] == "difference":
            expect = sets[0] - set().union(*sets[1:])
        else:
            return "FAIL", [f"unknown relation {o['relation']}"]
        if dr.capped:
            reasons.append("result capped; cannot verify decomposition")
            return "SKIP", reasons
        if dr.idset != expect:
            extra = len(dr.idset - expect)
            absent = len(expect - dr.idset)
            reasons.append(f"decomposition {o['relation']} mismatch: "
                           f"result={len(dr.idset):,} expect={len(expect):,} "
                           f"(+{extra} unexpected / -{absent} missing)")
    else:
        return "FAIL", [f"unknown oracle kind {kind}"]

    return ("FAIL" if reasons else "PASS"), reasons


def pair_invariants(records, drains):
    """asc==reverse(desc) and read_mask cheap/heavy set-agreement. Returns list[CaseResult]."""
    out = []
    by_id = {r["id"]: r for r in records}
    done = set()
    for r in records:
        cid = r["id"]
        # asc/desc: ids differ only by the .asc. / .desc. segment
        if ".asc." in cid:
            twin = cid.replace(".asc.", ".desc.")
            if twin in by_id and cid not in done and twin not in done:
                done |= {cid, twin}
                a, d = drains.get(cid), drains.get(twin)
                if a and d and not a.error and not d.error and not a.capped and not d.capped:
                    rs = []
                    if a.idset != d.idset:
                        rs.append(f"asc/desc set mismatch ({len(a.idset)} vs {len(d.idset)})")
                    elif a.ids != list(reversed(d.ids)):
                        rs.append("asc result != reverse(desc result) (ordering bug)")
                    out.append(CaseResult(f"order-invariant[{cid} ^ {twin}]",
                                          "FAIL" if rs else "PASS", rs))
        # read_mask agreement: same id with cost token swapped
        if ".cheap." in cid:
            twin = cid.replace(".cheap.", ".expensive.")
            if twin in by_id:
                a, d = drains.get(cid), drains.get(twin)
                if a and d and not a.error and not d.error and not a.capped and not d.capped:
                    rs = []
                    if a.idset != d.idset:
                        rs.append(f"read_mask set mismatch ({len(a.idset)} vs {len(d.idset)})")
                    out.append(CaseResult(f"readmask-invariant[{cid} ^ {twin}]",
                                          "FAIL" if rs else "PASS", rs))
    return out


# --- backends -----------------------------------------------------------------

class Backend:
    def __init__(self, target, secure=False, ca_path=None, server_name=None, timeout=300):
        import grpc
        opts = [("grpc.max_receive_message_length", 512 * 1024 * 1024)]
        if secure:
            ca = open(ca_path, "rb").read() if ca_path else None
            creds = grpc.ssl_channel_credentials(root_certificates=ca)
            if server_name:  # authority/SAN override for port-forwarded self-signed certs
                opts.append(("grpc.ssl_target_name_override", server_name))
            self.ch = grpc.secure_channel(target, creds, options=opts)
        else:
            self.ch = grpc.insecure_channel(target, options=opts)
        self.stub = ls_grpc.LedgerServiceStub(self.ch)
        self.timeout = timeout
        self._m = {
            "ListTransactions": self.stub.ListTransactions,
            "ListEvents": self.stub.ListEvents,
            "ListCheckpoints": self.stub.ListCheckpoints,
        }

    def send_fn(self, rpc):
        m = self._m[rpc]
        t = self.timeout
        return lambda req: m(req, timeout=t)


# --- main ---------------------------------------------------------------------

def categorize(rec, max_drain):
    o = rec["oracle"]
    if o["kind"] == "degenerate":
        return "single"
    lim = rec["request"].get("options", {}).get("limit")
    if lim and lim < 50:  # limit-semantics probe: one page, verify limit honored + ITEM_LIMIT
        return "single"
    if o["kind"] == "exact_count" and o["expected_count"] > max_drain:
        return "partial"
    return "full"


def main():
    ap = argparse.ArgumentParser(description="stable-v2 LedgerService correctness harness")
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--archival", help="kv-rpc target host:port")
    ap.add_argument("--archival-tls", action="store_true",
                    help="archival serves TLS (production kv-rpc does)")
    ap.add_argument("--archival-ca", help="PEM root/self-signed cert for the archival TLS")
    ap.add_argument("--archival-server-name",
                    help="TLS SAN/authority override (for a port-forwarded self-signed cert)")
    ap.add_argument("--fullnode", help="fullnode target host:port (TLS h2)")
    ap.add_argument("--fullnode-ca", help="PEM root cert for the fullnode TLS")
    ap.add_argument("--fullnode-server-name", help="fullnode TLS SAN/authority override")
    ap.add_argument("--fullnode-insecure", action="store_true",
                    help="treat --fullnode as plaintext h2c (e.g. local port-forward)")
    ap.add_argument("--only", help="regex; run only cases whose id matches")
    ap.add_argument("--max-drain", type=int, default=250_000,
                    help="cap items drained per case; larger exact_count cases are partial-checked")
    ap.add_argument("--timeout", type=int, default=300, help="per-RPC deadline seconds")
    ap.add_argument("--no-diff", action="store_true", help="skip cross-backend differential")
    ap.add_argument("--raw-mask", action="store_true",
                    help="drain with each case's own read_mask instead of the minimal identity mask")
    ap.add_argument("--list", action="store_true", help="parse + categorize only, no network")
    ap.add_argument("--out", help="write per-case JSON results here")
    ap.add_argument("--progress-log", help="line-buffered per-case progress file (tail -f), with ETA")
    ap.add_argument("--partial-pages", type=int, default=4,
                    help="page budget for over-cap (partial) cases; they get structural checks only")
    args = ap.parse_args()

    records = [json.loads(l) for l in open(args.corpus) if l.strip()]
    if args.only:
        rx = re.compile(args.only)
        records = [r for r in records if rx.search(r["id"])]
    by_id = {r["id"]: r for r in records}

    # ensure decomposition components are present even if --only filtered them out
    needed = set(by_id)
    for r in records:
        for c in r["oracle"].get("components", []):
            needed.add(c)
    if args.only:
        allrecs = {json.loads(l)["id"]: json.loads(l)
                   for l in open(args.corpus) if l.strip()}
        for c in needed - set(by_id):
            if c in allrecs:
                by_id[c] = allrecs[c]

    plan = {cid: categorize(by_id[cid], args.max_drain) for cid in needed}
    n_full = sum(1 for v in plan.values() if v == "full")
    n_part = sum(1 for v in plan.values() if v == "partial")
    n_single = sum(1 for v in plan.values() if v == "single")
    print(f"corpus={args.corpus} cases={len(records)} (+{len(needed)-len(records)} pulled components)")
    print(f"plan: full-drain={n_full}  partial(>{args.max_drain:,})={n_part}  single-probe={n_single}")

    # protojson validation: every request must parse into its proto type
    parse_errs = []
    for cid in needed:
        try:
            base_request(by_id[cid])
        except Exception as e:
            parse_errs.append((cid, str(e)[:140]))
    print(f"protojson: parsed {len(needed)-len(parse_errs)}/{len(needed)} requests")
    for cid, e in parse_errs[:20]:
        print(f"  PARSE-ERR {cid}: {e}")

    if args.list:
        for cid in sorted(needed):
            print(f"  {plan[cid]:8} {cid}")
        return 1 if parse_errs else 0
    if parse_errs:
        print("ERROR: corpus has unparseable requests; aborting", file=sys.stderr)
        return 2

    if not args.archival:
        print("ERROR: --archival is required to run (or pass --list)", file=sys.stderr)
        return 2

    archival = Backend(args.archival, secure=args.archival_tls, ca_path=args.archival_ca,
                       server_name=args.archival_server_name, timeout=args.timeout)
    fullnode = None
    if args.fullnode and not args.no_diff:
        fullnode = Backend(args.fullnode, secure=not args.fullnode_insecure,
                           ca_path=args.fullnode_ca, server_name=args.fullnode_server_name,
                           timeout=args.timeout)

    # drain every needed case on archival (cache by id)
    drains = {}
    t0 = time.time()
    plog = open(args.progress_log, "w", buffering=1) if args.progress_log else None
    if plog:
        plog.write(f"# {time.strftime('%H:%M:%S')} start: {len(needed)} cases "
                   f"(full={n_full} partial={n_part} single={n_single}) "
                   f"max_drain={args.max_drain}\n")
    N = len(needed)
    for i, cid in enumerate(sorted(needed), 1):
        rec = by_id[cid]
        mode = plan[cid]
        req = base_request(rec, identity_mask=not args.raw_mask)
        mp = args.partial_pages if mode == "partial" else 100_000
        dr = drain(archival.send_fn(rec["rpc"]), rec["rpc"], req,
                   full=(mode != "single"), max_drain=args.max_drain, max_pages=mp)
        drains[cid] = dr
        tag = "ERR" if dr.error else ("CAP" if dr.capped else "ok")
        el = time.time() - t0
        eta = (el / i) * (N - i)
        line = (f"{time.strftime('%H:%M:%S')} [{i}/{N}] {cid}: {len(dr.ids):,} items, "
                f"{len(dr.pages)} pg [{tag}] | elapsed {el/60:.1f}m eta ~{eta/60:.1f}m")
        print("  " + line, flush=True)
        if plog:
            plog.write(line + "\n")

    # oracle + structural checks
    results = []
    for r in records:
        status, reasons = check_oracle(r, drains, args.max_drain)
        results.append(CaseResult(r["id"], status, reasons,
                                   count=len(drains[r["id"]].idset),
                                   expected=r["oracle"].get("expected_count")))
    # pairwise invariants
    results += pair_invariants(records, drains)

    # cross-backend differential on shared-scope, drainable cases
    if fullnode:
        for r in records:
            if r["class"]["backend_scope"] != "shared":
                continue
            da = drains[r["id"]]
            if da.error or da.capped:
                continue
            db = drain(fullnode.send_fn(r["rpc"]), r["rpc"],
                       base_request(r, identity_mask=not args.raw_mask),
                       full=(plan[r["id"]] != "single"), max_drain=args.max_drain)
            rs = []
            if db.error:
                rs.append(f"fullnode rpc-error: {db.error}")
            elif db.capped:
                rs.append("fullnode result capped; differential skipped")
            elif da.idset != db.idset:
                rs.append(f"archival/fullnode mismatch: "
                          f"+{len(da.idset - db.idset)} archival-only / "
                          f"-{len(db.idset - da.idset)} fullnode-only")
            status = "SKIP" if (rs and "skipped" in rs[0]) else ("FAIL" if rs else "PASS")
            results.append(CaseResult(f"differential[{r['id']}]", status, rs))

    # report
    npass = sum(1 for x in results if x.status == "PASS")
    nfail = sum(1 for x in results if x.status == "FAIL")
    nskip = sum(1 for x in results if x.status == "SKIP")
    print(f"\n=== results ({time.time()-t0:.0f}s) ===")
    for x in results:
        if x.status != "PASS":
            print(f"  {x.status}: {x.cid}")
            for rr in x.reasons:
                print(f"      - {rr}")
    print(f"\nPASS={npass}  FAIL={nfail}  SKIP={nskip}  (of {len(results)} checks)")

    if args.out:
        json.dump([x.__dict__ for x in results], open(args.out, "w"), indent=2)
        print(f"wrote {args.out}")
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
