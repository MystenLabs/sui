import sys, os, json, random
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2alpha import ledger_service_pb2 as ls, ledger_service_pb2_grpc as lsg, query_options_pb2 as qo
from google.protobuf import json_format

REQ = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "load.mainnet.jsonl")
lines = [json.loads(l) for l in open(REQ) if l.strip()]
random.seed(0)
sample = random.sample(lines, 40)

stub = lsg.LedgerServiceStub(grpc.insecure_channel(
    "localhost:19000", options=[("grpc.max_receive_message_length", 512*1024*1024)]))
CTOR = {"ListTransactions": ls.ListTransactionsRequest,
        "ListEvents": ls.ListEventsRequest,
        "ListCheckpoints": ls.ListCheckpointsRequest}
STUBM = {"ListTransactions": stub.ListTransactions,
         "ListEvents": stub.ListEvents,
         "ListCheckpoints": stub.ListCheckpoints}

nonempty = 0; empty = 0; errs = 0
by_tier = {}
for rec in sample:
    req = json_format.ParseDict(rec["request"], CTOR[rec["rpc"]]())
    n = 0
    try:
        for resp in STUBM[rec["rpc"]](req):
            if resp.WhichOneof("response") == "item":
                n += 1
            if n >= 50:
                break
    except grpc.RpcError as e:
        errs += 1; print(f"  ERR {rec['rpc']:16} {rec['dim']}/{rec['tier']}: {e.code().name}"); continue
    d = by_tier.setdefault(rec["tier"], [0, 0])
    if n > 0: nonempty += 1; d[0] += 1
    else: empty += 1; d[1] += 1
print(f"\nsampled {len(sample)}: nonempty={nonempty} empty={empty} errs={errs}")
print("per-tier [nonempty, empty]:", by_tier)
