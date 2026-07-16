"""Chain arbiter for the +9: digests the RPC returns for destroy_zero that the
Snowflake MOVE_CALL table lacks (or vice versa), then fetch each from the chain
via GetTransaction and read its actual Move calls. The ledger decides."""
import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H
from sui.rpc.v2 import ledger_service_pb2 as ls
from sui.rpc.v2 import ledger_service_pb2_grpc as ls_grpc
from sui.rpc.v2 import query_options_pb2 as qo
from google.protobuf import json_format

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.testnet.jsonl")}
txreq = recs["tx.move_call.full.dense_everywhere.shared.expensive.asc.95308b"]["request"]
b = H.Backend("localhost:18000", secure=True, ca_path="kvrpc.testnet.crt",
              server_name="kv-rpc-http2.rpc-kv-testnet.svc.cluster.local", timeout=120)

# 1. RPC digest set
base = json_format.ParseDict(txreq, ls.ListTransactionsRequest())
base.ClearField("read_mask"); base.read_mask.paths.append("digest")
last = None; R = set()
for _ in range(100000):
    r = ls.ListTransactionsRequest(); r.CopyFrom(base)
    if last is not None: r.options.after = last
    end = None; cur = None
    for resp in b.send_fn("ListTransactions")(r):
        payload = H.response_payload("ListTransactions", resp)
        if payload is not None:
            R.add(payload.digest)
        if resp.HasField("watermark") and resp.watermark.cursor:
            cur = resp.watermark.cursor
        if resp.HasField("end"):
            end = resp.end.reason
    if end in (qo.QUERY_END_REASON_ITEM_LIMIT, qo.QUERY_END_REASON_SCAN_LIMIT) and cur and cur != last:
        last = cur; continue
    break

# 2. Snowflake digest set
S = set()
for line in open("/tmp/sf_digests.csv").read().splitlines()[1:]:
    d = line.strip().strip('"')
    if d:
        S.add(d)

extra = sorted(R - S)   # RPC-only
missing = sorted(S - R)  # Snowflake-only
print(f"RPC={len(R)}  Snowflake={len(S)}  RPC_only={len(extra)}  SF_only={len(missing)}")

# 3. chain arbiter: fetch each divergent digest, read its Move calls
v2 = ls_grpc.LedgerServiceStub(b.ch)


def move_calls(digest):
    req = ls.GetTransactionRequest(digest=digest)
    req.read_mask.paths.append("transaction")
    resp = v2.GetTransaction(req, timeout=30)
    td = json_format.MessageToDict(resp.transaction)
    cmds = (td.get("transaction", {}).get("kind", {})
            .get("programmableTransaction", {}).get("commands", []))
    return [(c["moveCall"].get("package"), c["moveCall"].get("module"), c["moveCall"].get("function"))
            for c in cmds if "moveCall" in c]


for tag, digs in (("RPC_ONLY", extra), ("SF_ONLY", missing)):
    for d in digs[:15]:
        try:
            mc = move_calls(d)
            hit = any(m[1] == "coin" and m[2] == "destroy_zero" for m in mc)
            print(f"{tag} {d} calls_destroy_zero={hit} move_calls={mc}")
        except Exception as e:
            print(f"{tag} {d} GetTransaction ERR {str(e)[:90]}")
