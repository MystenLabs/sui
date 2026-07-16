import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H
from sui.rpc.v2 import ledger_service_pb2 as ls
from sui.rpc.v2 import query_options_pb2 as qo
from google.protobuf import json_format

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.testnet.jsonl")}
cid = sys.argv[1] if len(sys.argv) > 1 else "cp.move_call.full.dense_everywhere.shared.expensive.asc.95308b"
mask = sys.argv[2] if len(sys.argv) > 2 else "identity"   # "identity" | "raw"
rec = recs[cid]
rpc = rec["rpc"]
REQT = {"ListCheckpoints": ls.ListCheckpointsRequest, "ListTransactions": ls.ListTransactionsRequest,
        "ListEvents": ls.ListEventsRequest}[rpc]
base = json_format.ParseDict(rec["request"], REQT())
if mask == "identity":
    base.ClearField("read_mask")
    base.read_mask.paths.extend(H.IDENTITY_MASK[rpc])
print(f"{cid} mask={mask} read_mask={list(base.read_mask.paths)} "
      f"range {rec['request'].get('start_checkpoint',0)}->{rec['request']['end_checkpoint']} expected={rec['oracle'].get('expected_count')}")
b = H.Backend("localhost:18000", secure=True, ca_path="kvrpc.testnet.crt",
              server_name="kv-rpc-http2.rpc-kv-testnet.svc.cluster.local", timeout=120)
last = None; seen = set(); asc = base.options.ordering == qo.ORDERING_ASCENDING
term = None
for pg in range(5000):
    r = REQT(); r.CopyFrom(base)
    if last is not None:
        setattr(r.options, "after" if asc else "before", last)
    items = 0; end = None; cur = None; first = None; lastseq = None
    for resp in b.send_fn(rpc)(r):
        payload = H.response_payload(rpc, resp)
        if payload is not None:
            items += 1
            s = payload.sequence_number if rpc == "ListCheckpoints" else None
            if s is not None:
                if first is None: first = s
                lastseq = s
                seen.add(s)
        if resp.HasField("watermark") and resp.watermark.cursor:
            cur = resp.watermark.cursor
        if resp.HasField("end"):
            end = qo.QueryEndReason.Name(resp.end.reason)
    if pg % 25 == 0 or end not in ("QUERY_END_REASON_ITEM_LIMIT", "QUERY_END_REASON_SCAN_LIMIT"):
        print(f"pg{pg}: items={items} seq[{first}..{lastseq}] end={end} cum={len(seen)}")
    if end in ("QUERY_END_REASON_ITEM_LIMIT", "QUERY_END_REASON_SCAN_LIMIT") and cur and cur != last:
        last = cur; continue
    term = end; break
print(f"DONE total_distinct={len(seen)} terminal={term}")
