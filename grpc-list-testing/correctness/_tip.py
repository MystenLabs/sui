import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H
from sui.rpc.v2alpha import ledger_service_pb2 as ls
from sui.rpc.v2alpha import query_options_pb2 as qo

b = H.Backend("localhost:18000", secure=True, ca_path="kvrpc.testnet.crt",
              server_name="kv-rpc-http2.rpc-kv-testnet.svc.cluster.local", timeout=60)
req = ls.ListCheckpointsRequest()           # default read_mask = "sequence_number,digest"
req.options.ordering = qo.ORDERING_DESCENDING
req.options.limit_items = 1
for resp in b.send_fn("ListCheckpoints")(req):
    w = resp.WhichOneof("response")
    if w == "item":
        print("TIP checkpoint =", resp.item.checkpoint.sequence_number)
        break
    if w == "watermark":
        print("watermark cp_lo =", resp.watermark.checkpoint_lo)
    if w == "end":
        print("end:", qo.QueryEndReason.Name(resp.end.reason)); break
