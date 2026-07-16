"""Internal-consistency proof: the checkpoints ListCheckpoints(F) returns must
equal the distinct checkpoints of the transactions ListTransactions(F) returns.
Uses only the RPC -- no warehouse, no chain."""
import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H
from sui.rpc.v2 import ledger_service_pb2 as ls
from sui.rpc.v2 import query_options_pb2 as qo
from google.protobuf import json_format

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.testnet.jsonl")}
txreq = recs["tx.move_call.full.dense_everywhere.shared.expensive.asc.95308b"]["request"]
cpreq = recs["cp.move_call.full.dense_everywhere.shared.expensive.asc.95308b"]["request"]
b = H.Backend("localhost:18000", secure=True, ca_path="kvrpc.testnet.crt",
              server_name="kv-rpc-http2.rpc-kv-testnet.svc.cluster.local", timeout=120)


def drain(rpc, reqdict, mask, take):
    REQT = {"ListTransactions": ls.ListTransactionsRequest, "ListCheckpoints": ls.ListCheckpointsRequest}[rpc]
    base = json_format.ParseDict(reqdict, REQT())
    base.ClearField("read_mask")
    for p in mask.split(","):
        base.read_mask.paths.append(p)
    last = None; got = []; 
    asc = base.options.ordering == qo.ORDERING_ASCENDING
    for _ in range(100000):
        r = REQT(); r.CopyFrom(base)
        if last is not None:
            setattr(r.options, "after" if asc else "before", last)
        end = None; cur = None
        for resp in b.send_fn(rpc)(r):
            payload = H.response_payload(rpc, resp)
            if payload is not None:
                got.append(take(payload))
            if resp.HasField("watermark") and resp.watermark.cursor:
                cur = resp.watermark.cursor
            if resp.HasField("end"):
                end = resp.end.reason
        if end in (qo.QUERY_END_REASON_ITEM_LIMIT, qo.QUERY_END_REASON_SCAN_LIMIT) and cur and cur != last:
            last = cur; continue
        break
    return got


tx_cps = drain("ListTransactions", txreq, "digest,checkpoint", lambda it: it.checkpoint)
cp_seqs = drain("ListCheckpoints", cpreq, "sequence_number", lambda it: it.sequence_number)
tx_cp_set, cp_set = set(tx_cps), set(cp_seqs)
print(f"ListTransactions: {len(tx_cps):,} txns across {len(tx_cp_set):,} distinct checkpoints")
print(f"ListCheckpoints : {len(cp_seqs):,} items, {len(cp_set):,} distinct checkpoints")
print(f"cp_set subset of tx-derived? {cp_set <= tx_cp_set}")
print(f"checkpoints with a matching tx that ListCheckpoints OMITTED: {len(tx_cp_set - cp_set):,}")
print(f"checkpoints ListCheckpoints returned that have NO matching tx: {len(cp_set - tx_cp_set):,}")
