import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2 import ledger_service_pb2 as ls, ledger_service_pb2_grpc as lsg, query_options_pb2 as qo

stub = lsg.LedgerServiceStub(grpc.insecure_channel(
    "localhost:19000", options=[("grpc.max_receive_message_length", 512*1024*1024)]))

# Unfiltered ListTransactions at increasing starts; report items, terminal, and the
# watermark checkpoint the scan reached (reveals the indexed frontier).
for start in (285_600_000, 286_000_000, 286_500_000, 287_000_000, 290_000_000, 293_000_000):
    n=0; end=None; wm_checkpoint=None; cur=None
    try:
        for resp in stub.ListTransactions(ls.ListTransactionsRequest(
                start_checkpoint=start, options=qo.QueryOptions(limit=5))):
            if resp.HasField("transaction"):
                n += 1
            if resp.HasField("watermark"):
                if resp.watermark.HasField("checkpoint"):
                    wm_checkpoint = resp.watermark.checkpoint
                if resp.watermark.cursor:
                    cur = resp.watermark.cursor
            if resp.HasField("end"):
                end = qo.QueryEndReason.Name(resp.end.reason)
            if n>=5: break
        print(f"start={start:>11}  items={n}  reached_cp={wm_checkpoint}  end={end}  cursor={'set' if cur else None}")
    except grpc.RpcError as e:
        print(f"start={start:>11}  {e.code().name}: {str(e.details())[:50]}")
