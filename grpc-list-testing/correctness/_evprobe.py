#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Pin down ev.sender=0: real RPC/index bug vs corpus-oracle artifact.

Metamorphic, no warehouse. The authoritative per-txn event count comes from
GetTransaction (primary-key fetch of the full ExecutedTransaction record), which
is independent of the event Sender bitmap dimension that ListEvents uses.

  VALIDATE: prove the events-extraction path works -- pull a real event from an
    unfiltered ListEvents window, GetTransaction its txn, assert events>0.
  PER SENDER: ListEvents(sender=S) count + terminal, then GetTransaction a sample
    of S's transactions (digests from ListTransactions(sender=S)) and count events.
      S's txns emit events but ListEvents=0  -> RPC/index BUG (event Sender dim broken)
      S's txns emit zero events              -> ORACLE bug (expected counted txns)
"""
import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H
from sui.rpc.v2alpha import ledger_service_pb2 as lsa
from sui.rpc.v2alpha import query_options_pb2 as qo
from sui.rpc.v2 import ledger_service_pb2 as lv2
from sui.rpc.v2 import ledger_service_pb2_grpc as lv2grpc
from google.protobuf import json_format

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.testnet.jsonl") if l.strip()}
CEIL = json.load(open("../manifest.testnet.json"))["cp_ceiling"]
b = H.Backend("localhost:18000", secure=True, ca_path="kvrpc.testnet.crt",
              server_name="kv-rpc-http2.rpc-kv-testnet.svc.cluster.local", timeout=300)
_gt = lv2grpc.LedgerServiceStub(b.ch)


def gt_events(digest):
    req = lv2.GetTransactionRequest(digest=digest)
    req.read_mask.paths.extend(["digest", "events"])
    resp = _gt.GetTransaction(req, timeout=60)
    return len(json_format.MessageToDict(resp.transaction).get("events", {}).get("events", []))


def drain_collect(rpc, req, get_id, cap, page_budget=50):
    """Drain rpc, collecting get_id(item); returns (ids, terminal)."""
    asc = req.options.ordering == qo.ORDERING_ASCENDING
    last = None; ids = []
    for _ in range(page_budget):
        r = type(req)(); r.CopyFrom(req)
        if last is not None:
            setattr(r.options, "after" if asc else "before", last)
        end = None; cur = None
        for resp in b.send_fn(rpc)(r):
            w = resp.WhichOneof("response")
            if w == "item":
                ids.append(get_id(resp.item))
                if resp.item.watermark.cursor: cur = resp.item.watermark.cursor
            elif w == "watermark" and resp.watermark.cursor: cur = resp.watermark.cursor
            elif w == "end": end = resp.end.reason
        if len(ids) >= cap: return (ids, "CAP")
        if end in (qo.QUERY_END_REASON_ITEM_LIMIT, qo.QUERY_END_REASON_SCAN_LIMIT) and cur and cur != last:
            last = cur; continue
        return (ids, qo.QueryEndReason.Name(end) if end is not None else "NO_END")
    return (ids, "PAGE_BUDGET")


# ---- VALIDATE the events-extraction path on a real event-emitter -----------------
un = lsa.ListEventsRequest()
un.start_checkpoint = CEIL - 200_000; un.end_checkpoint = CEIL
un.options.limit_items = 10; un.options.ordering = qo.ORDERING_DESCENDING
d0 = None
for resp in b.send_fn("ListEvents")(un):
    if resp.WhichOneof("response") == "item":
        d0 = resp.item.transaction_digest; break
e0 = gt_events(d0) if d0 else -1
print(f"VALIDATE events path: unfiltered ListEvents -> tx {str(d0)[:18]}.. GetTransaction events={e0} "
      f"({'OK, path works' if e0 > 0 else 'BROKEN PATH -- results below are not trustworthy'})")

# ---- per sender -----------------------------------------------------------------
for suffix in ["872104", "cbe60f"]:
    evid = next((i for i in recs if i.startswith("ev.sender") and i.endswith(suffix)), None)
    evrec = recs[evid]
    addr = evrec["request"]["filter"]["terms"][0]["literals"][0]["include"]["sender"]["address"]
    print(f"\n===== {evid} =====\nsender: {addr}")

    evreq = json_format.ParseDict(evrec["request"], lsa.ListEventsRequest())
    ev_ids, ev_end = drain_collect("ListEvents", evreq, lambda it: it.transaction_digest, cap=80000, page_budget=600)
    print(f"ListEvents(sender)        -> {len(ev_ids):>6} events   terminal={ev_end}")

    # digests of S's transactions (cheap: digest mask), sample for GetTransaction
    txd = dict(evrec["request"]); txd.pop("read_mask", None); txd["readMask"] = "digest"
    txreq = json_format.ParseDict(txd, lsa.ListTransactionsRequest(), ignore_unknown_fields=True)
    tx_ids, tx_end = drain_collect("ListTransactions", txreq, lambda it: it.transaction.digest, cap=1000)
    sample = tx_ids[:: max(1, len(tx_ids) // 60)][:60]
    emitted = sum(1 for d in sample if gt_events(d) > 0)
    tot = sum(gt_events(d) for d in sample)
    print(f"ListTransactions(sender)  -> {len(tx_ids):>6} txns     terminal={tx_end}")
    print(f"   GetTransaction on {len(sample)} sampled txns: {emitted} emitted events, {tot} events total")

    if tot > 0 and len(ev_ids) == 0:
        print(f"   >>> RPC/INDEX BUG: S's txns emit events but ListEvents(sender)=0.")
    elif tot == 0:
        print(f"   >>> ORACLE BUG: S's sampled txns emit ZERO events; ListEvents(sender)=0 is CORRECT. "
              f"Corpus expected_count counted transactions, not events.")
    else:
        print(f"   >>> inconclusive (ListEvents={len(ev_ids)}, sampled tx events={tot})")
