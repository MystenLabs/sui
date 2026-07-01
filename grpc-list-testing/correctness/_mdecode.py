import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2alpha import ledger_service_pb2 as ls, ledger_service_pb2_grpc as lsg
from google.rpc import status_pb2
import harness as H

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.mainnet.jsonl")}
stub = lsg.LedgerServiceStub(grpc.insecure_channel(
    "localhost:19000", options=[("grpc.max_receive_message_length", 512*1024*1024)]))

for cid in ["tx.sender_not_move_call.anchored.shared",
            "tx.degenerate.dense_and_dense.empty.archival"]:
    r = recs[cid]
    req = H.base_request(r, identity_mask=True)
    print(f"\n========== {cid} ==========")
    try:
        for _ in stub.ListTransactions(req):
            pass
    except grpc.RpcError as e:
        print(f"code         : {e.code()} ({e.code().value[0]})")
        print(f"details (full):\n  " + "\n  ".join(str(e.details()).splitlines()))
        for k, v in (e.trailing_metadata() or []):
            if k == "grpc-status-details-bin":
                st = status_pb2.Status.FromString(v)
                print(f"google.rpc.Status.code   : {st.code}")
                print(f"google.rpc.Status.message:\n  " + "\n  ".join(st.message.splitlines()))
                print(f"google.rpc.Status.details: {len(st.details)} entries")
                for d in st.details:
                    print(f"  - type_url={d.type_url} ({len(d.value)}b)")
        dbg = e.debug_error_string()
        print(f"debug_error_string:\n  {dbg}")
