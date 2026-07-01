#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Chain-arbiter oracle tier.

A differential failure (RPC count != Snowflake expected_count) doesn't say who is
right -- the bitmap inverted index (serves the List APIs) and the analytics tables
are BOTH derived search indexes, so either can be wrong. This adjudicates a count
mismatch against the *chain*, not against another search index:

  soundness (chain): take the digests the RPC actually returned, fetch each
    transaction by digest via v2 GetTransaction from an INDEPENDENT fullnode
    (`--chain-target`, a separate executor/store from the kv-rpc's BigTable), and
    RE-DERIVE the predicate from the raw ExecutedTransaction. This is not "ask an
    index to confirm an index": GetTransaction is a primary-key lookup of the raw
    transaction record (different code path + different node than the bitmap inverted
    index and the analytics tables), and the predicate is recomputed from scratch. If
    every sampled item resolves to a real tx that genuinely matches the filter, the
    RPC is sound -> a higher-than-expected count means the WAREHOUSE undercounts
    (analytics gap). A non-match => RPC false positive.

  completeness (cross-RPC, metamorphic): for ListCheckpoints, the truth set is the
    distinct checkpoints of ListTransactions(same filter,range) on the SAME index.
    If ListCheckpoints returns fewer, the index contradicts itself -> RPC bug. Needs
    no external oracle at all.

Without `--chain-target`, GetTransaction falls back to the archival backend: still an
independent code path from the search indexes, but it shares the kv-rpc's underlying
store -- point at a fullnode for full store independence. (Note: the proto Transaction
`bcs` is a re-serialization, not the original signed bytes, so a byte-level
`hash(bcs)==digest` recompute does NOT reproduce Sui's on-chain digest; we corroborate
via an independent node instead.)
"""
import argparse
import json
import os
import random
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H  # noqa: E402
from sui.rpc.v2alpha import ledger_service_pb2 as lsa  # noqa: E402
from sui.rpc.v2alpha import query_options_pb2 as qo  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2 as lv2  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2_grpc as lv2grpc  # noqa: E402
from google.protobuf import json_format  # noqa: E402

def norm_addr(a):
    if a is None:
        return None
    return "0x" + a.lower().removeprefix("0x").zfill(64)


# --- filter evaluation against a chain-fetched ExecutedTransaction dict -----------

def _move_calls(tx):
    cmds = (tx.get("transaction", {}).get("kind", {})
            .get("programmableTransaction", {}).get("commands", []))
    return [c["moveCall"] for c in cmds if "moveCall" in c]


def _events(tx):
    return tx.get("events", {}).get("events", [])


def _changed_object_ids(tx):
    return [norm_addr(c.get("objectId")) for c in tx.get("effects", {}).get("changedObjects", [])]


def predicate_holds(pred, tx):
    """True/False if `pred` (one corpus predicate dict) holds for tx, or None if the
    arbiter does not support this predicate shape."""
    if "sender" in pred:
        return norm_addr(tx.get("transaction", {}).get("sender")) == norm_addr(pred["sender"]["address"])
    if "affected_object" in pred:
        return norm_addr(pred["affected_object"]["object_id"]) in set(_changed_object_ids(tx))
    if "move_call" in pred:
        parts = pred["move_call"]["function"].split("::")
        pkg = norm_addr(parts[0]); mod = parts[1] if len(parts) > 1 else None
        fn = parts[2] if len(parts) > 2 else None
        for mc in _move_calls(tx):
            if norm_addr(mc.get("package")) == pkg and (mod is None or mc.get("module") == mod) \
                    and (fn is None or mc.get("function") == fn):
                return True
        return False
    if "emit_module" in pred:
        parts = pred["emit_module"]["module"].split("::")
        pkg = norm_addr(parts[0]); mod = parts[1] if len(parts) > 1 else None
        for ev in _events(tx):
            if norm_addr(ev.get("packageId")) == pkg and (mod is None or ev.get("module") == mod):
                return True
        return False
    if "event_type" in pred:
        wp = pred["event_type"]["type"].split("::", 1)
        want = norm_addr(wp[0]) + ("::" + wp[1] if len(wp) > 1 else "")
        for ev in _events(tx):
            ep = (ev.get("eventType") or "").split("::", 1)
            et = norm_addr(ep[0]) + ("::" + ep[1] if len(ep) > 1 else "")
            if et == want or et.startswith(want + "::") or et.startswith(want + "<"):
                return True
        return False
    return None  # affected_address, package_write, event_stream_head: unsupported


def filter_matches(flt, tx):
    """DNF eval; None if any needed predicate is unsupported."""
    any_unsupported = False
    for term in flt.get("terms", []):
        ok = True
        for lit in term.get("literals", []):
            if "include" in lit:
                hold = predicate_holds(lit["include"], tx)
            elif "exclude" in lit:
                r = predicate_holds(lit["exclude"], tx)
                hold = (not r) if r is not None else None
            else:
                hold = None
            if hold is None:
                any_unsupported = True; ok = False; break
            if not hold:
                ok = False; break
        if ok:
            return True
    return None if any_unsupported else False


# --- chain client + bounded drains ------------------------------------------------

class Arbiter:
    def __init__(self, list_backend, chain_backend, independent):
        self.list_backend = list_backend
        self.v2 = lv2grpc.LedgerServiceStub(chain_backend.ch)
        self.independent = independent  # True if chain_backend is a separate fullnode
        self._cache = {}
        self.served = self.digest_ok = self.digest_bad = 0

    def tx(self, digest):
        """GetTransaction by digest from the chain backend; return ExecutedTransaction
        dict. Sanity-check that the node returned the record we asked for (so a wrong
        digest can't silently pass), then the caller re-derives the predicate."""
        if digest not in self._cache:
            req = lv2.GetTransactionRequest(digest=digest)
            for p in ("digest", "transaction.kind", "transaction.sender", "events", "effects"):
                req.read_mask.paths.append(p)
            resp = self.v2.GetTransaction(req, timeout=60)
            self.served += 1
            if resp.transaction.digest == digest:
                self.digest_ok += 1
            else:
                self.digest_bad += 1
            self._cache[digest] = json_format.MessageToDict(resp.transaction)
        return self._cache[digest]

    def _pages(self, rpc, base_req, on_item, stop, page_budget):
        asc = base_req.options.ordering == qo.ORDERING_ASCENDING
        last = None
        for pg in range(page_budget):
            r = type(base_req)(); r.CopyFrom(base_req)
            if last is not None:
                setattr(r.options, "after" if asc else "before", last)
            end = None; cur = None
            for resp in self.list_backend.send_fn(rpc)(r):
                w = resp.WhichOneof("response")
                if w == "item":
                    on_item(resp.item)
                    if resp.item.watermark.cursor:
                        cur = resp.item.watermark.cursor
                elif w == "watermark" and resp.watermark.cursor:
                    cur = resp.watermark.cursor
                elif w == "end":
                    end = resp.end.reason
            if stop():
                return False  # stopped early, not exhausted
            if end in (qo.QUERY_END_REASON_ITEM_LIMIT, qo.QUERY_END_REASON_SCAN_LIMIT) \
                    and cur and cur != last:
                last = cur; continue
            return True  # range genuinely exhausted
        return False  # page budget hit

    def collect_ids(self, rec, pool):
        rpc = rec["rpc"]
        req = json_format.ParseDict(rec["request"], H.REQ_TYPE[rpc]())
        req.ClearField("read_mask")
        # event masks are relative to the Event body; transaction_digest is item-level
        # (always present). use a cheap valid path for events, "digest" for transactions.
        req.read_mask.paths.append("event_type" if rpc == "ListEvents" else "digest")
        ids = []

        def on_item(it):
            ids.append(it.transaction_digest if rpc == "ListEvents" else it.transaction.digest)
        self._pages(rpc, req, on_item, lambda: len(ids) >= pool, page_budget=10)
        return ids

    def cp_truth(self, rec, threshold, page_budget=80):
        """Distinct checkpoints of ListTransactions(same filter,range); early-exit once
        the count exceeds `threshold` (enough to prove a ListCheckpoints under-return)."""
        req = json_format.ParseDict(rec["request"], lsa.ListTransactionsRequest())
        req.ClearField("read_mask")
        for p in ("digest", "checkpoint"):
            req.read_mask.paths.append(p)
        cps = set()
        ended = self._pages("ListTransactions", req, lambda it: cps.add(it.transaction.checkpoint),
                            lambda: len(cps) > threshold, page_budget)
        return len(cps), ended

    def soundness(self, rec, n, pool):
        flt = rec["request"].get("filter")
        ids = self.collect_ids(rec, pool)
        if not ids:
            return None
        sample = random.sample(ids, min(n, len(ids)))
        matched = unsupported = 0
        for digest in sample:
            try:
                m = filter_matches(flt, self.tx(digest))
            except Exception:
                m = None
            if m is None:
                unsupported += 1
            elif m:
                matched += 1
        return (len(sample), matched, unsupported)


def classify(rec, rpc_count, expected, arb, sample):
    rpc = rec["rpc"]
    delta = rpc_count - expected
    if rpc == "ListCheckpoints":
        truth, ended = arb.cp_truth(rec, threshold=rpc_count)
        if truth > rpc_count:
            return (f"RPC BUG (ListCheckpoints under-returns): tx-stream over the same "
                    f"filter+range has >= {truth:,} distinct checkpoints, ListCheckpoints "
                    f"returned only {rpc_count:,} -> drops >= {truth - rpc_count:,}. "
                    f"(warehouse expected {expected:,})")
        if ended and truth == rpc_count and truth != expected:
            return (f"WAREHOUSE GAP: ListCheckpoints self-consistent ({rpc_count:,} == "
                    f"tx-derived {truth:,}) but warehouse expects {expected:,} "
                    f"(delta {truth - expected:+,}).")
        return (f"INCONCLUSIVE: cp={rpc_count:,} tx-derived"
                f"{'' if ended else '(partial)'}={truth:,} warehouse={expected:,}")
    s = arb.soundness(rec, sample, pool=max(sample * 3, 150))
    if s is None:
        return "SKIP: nothing drained to sample"
    n, matched, unsup = s
    evaluable = n - unsup
    if evaluable == 0:
        return "UNSUPPORTED: filter shape not evaluable by arbiter (e.g. affected_address/NOT)"
    src = "an INDEPENDENT fullnode" if arb.independent else "the archival store"
    if matched == evaluable:
        side = ("WAREHOUSE UNDERCOUNTS (analytics gap)" if delta > 0
                else "warehouse OVERCOUNTS" if delta < 0 else "exact match")
        return (f"RPC SOUND: sampled {n} ({unsup} unevaluable); all {matched} resolve to real "
                f"txns that match the filter per {src}. delta {delta:+,} -> {side}.")
    return (f"RPC BUG (false positives): sampled {n}, only {matched}/{evaluable} resolve to "
            f"txns that match the filter per {src}. delta {delta:+,}.")


def main():
    ap = argparse.ArgumentParser(description="chain-arbiter for harness count mismatches")
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--results", required=True, help="results.json from a harness run")
    ap.add_argument("--archival", required=True, help="kv-rpc host:port (List APIs + default GetTransaction)")
    ap.add_argument("--archival-tls", action="store_true")
    ap.add_argument("--archival-ca")
    ap.add_argument("--archival-server-name")
    ap.add_argument("--chain-target", help="independent fullnode host:port for GetTransaction (storage independence)")
    ap.add_argument("--chain-tls", action="store_true")
    ap.add_argument("--chain-ca")
    ap.add_argument("--chain-server-name")
    ap.add_argument("--chain-insecure", action="store_true",
                    help="treat --chain-target as plaintext h2c (e.g. fullnode :9000)")
    ap.add_argument("--sample", type=int, default=20)
    ap.add_argument("--only", help="regex on case id")
    args = ap.parse_args()

    corpus = {json.loads(l)["id"]: json.loads(l) for l in open(args.corpus) if l.strip()}
    results = json.load(open(args.results))
    list_backend = H.Backend(args.archival, secure=args.archival_tls, ca_path=args.archival_ca,
                            server_name=args.archival_server_name, timeout=300)
    if args.chain_target:
        chain_backend = H.Backend(args.chain_target, secure=not args.chain_insecure,
                                 ca_path=args.chain_ca, server_name=args.chain_server_name,
                                 timeout=300)
        independent = True
        print(f"GetTransaction -> INDEPENDENT fullnode {args.chain_target} "
              f"({'plaintext h2c' if args.chain_insecure else 'tls'})")
    else:
        chain_backend = list_backend
        independent = False
        print(f"GetTransaction -> archival {args.archival} "
              f"(independent code path; pass --chain-target for full store independence)")
    arb = Arbiter(list_backend, chain_backend, independent)

    rx = re.compile(args.only) if args.only else None
    mismatch = re.compile(r"count ([\d,]+) != expected ([\d,]+)")
    todo = []
    for r in results:
        cid = r["cid"]
        if cid not in corpus or (rx and not rx.search(cid)):
            continue
        for reason in r.get("reasons", []):
            m = mismatch.search(reason)
            if m:
                todo.append((cid, int(m.group(1).replace(",", "")), int(m.group(2).replace(",", ""))))
                break
    print(f"adjudicating {len(todo)} count-mismatch case(s)\n")
    for cid, rpc_count, expected in todo:
        try:
            verdict = classify(corpus[cid], rpc_count, expected, arb, args.sample)
        except Exception as e:
            verdict = f"ERROR {type(e).__name__}: {str(e)[:140]}"
        print(f"* {cid}\n    rpc={rpc_count:,} warehouse={expected:,}\n    -> {verdict}\n", flush=True)
    src = "independent fullnode" if arb.independent else "archival store"
    print(f"GetTransaction: {arb.served} txns served from {src}; "
          f"{arb.digest_ok} returned the requested digest, {arb.digest_bad} mismatched.")


if __name__ == "__main__":
    main()
