import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2alpha import ledger_service_pb2 as ls
from sui.rpc.v2alpha import ledger_service_pb2_grpc as lsg
from sui.rpc.v2alpha import query_options_pb2 as qo

ch = grpc.insecure_channel("localhost:19000",
        options=[("grpc.max_receive_message_length", 512 * 1024 * 1024)])
stub = lsg.LedgerServiceStub(ch)

def drain(name, call, start):
    n = 0; reason = None; lo = hi = None
    try:
        for resp in call:
            w = resp.WhichOneof("response")
            if w == "item":
                n += 1
            elif w == "end":
                reason = qo.QueryEndReason.Name(resp.end.reason); break
            if n >= 5: break
        print(f"  {name:16} start={start:>12} -> SERVED  items={n} end={reason}")
        return "served"
    except grpc.RpcError as e:
        print(f"  {name:16} start={start:>12} -> {e.code().name}: {str(e.details())[:60]}")
        return e.code().name

for start in (285_600_000, 287_000_000, 290_000_000, 293_100_000):
    print(f"--- checkpoint {start:,} ---")
    drain("ListCheckpoints", stub.ListCheckpoints(
        ls.ListCheckpointsRequest(start_checkpoint=start,
            options=qo.QueryOptions(limit_items=5))), start)
    drain("ListTransactions", stub.ListTransactions(
        ls.ListTransactionsRequest(start_checkpoint=start,
            options=qo.QueryOptions(limit_items=5))), start)
    drain("ListEvents", stub.ListEvents(
        ls.ListEventsRequest(start_checkpoint=start,
            options=qo.QueryOptions(limit_items=5))), start)
