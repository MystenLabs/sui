import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2alpha import ledger_service_pb2 as ls, ledger_service_pb2_grpc as lsg, query_options_pb2 as qo
import harness as H

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.mainnet.jsonl")}
stub = lsg.LedgerServiceStub(grpc.insecure_channel(
    "localhost:19000", options=[("grpc.max_receive_message_length", 512*1024*1024)]))

for cid in ["tx.sender_not_move_call.anchored.shared",
            "tx.degenerate.dense_and_dense.empty.archival"]:
    r = recs[cid]
    req = H.base_request(r, identity_mask=True)
    n_items = n_wm = 0; last_cursor = None; end = None
    print(f"\n=== {cid} ===")
    try:
        for resp in stub.ListTransactions(req):
            which = resp.WhichOneof("response")
            if which == "item":
                n_items += 1
                if resp.item.watermark.cursor: last_cursor = resp.item.watermark.cursor
            elif which == "watermark":
                n_wm += 1
                if resp.watermark.cursor: last_cursor = resp.watermark.cursor
            elif which == "end":
                end = qo.QueryEndReason.Name(resp.end.reason)
        print(f"  clean: items={n_items} wm={n_wm} "
              f"cursor={'SET('+str(len(last_cursor))+'b)' if last_cursor else None} end={end}")
    except grpc.RpcError as e:
        print(f"  items_before_err={n_items} wm_frames={n_wm} "
              f"cursor_seen={'YES('+str(len(last_cursor))+'b)' if last_cursor else 'NONE'}")
        print(f"  code={e.code().name}")
        print(f"  details={str(e.details())[:160]}")
        tm = list(e.trailing_metadata() or [])
        print(f"  trailing_metadata={[(k, (v[:40] if isinstance(v,str) else '<bin %db>'%len(v))) for k,v in tm]}")
