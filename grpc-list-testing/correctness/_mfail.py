import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import harness as H

recs = {json.loads(l)["id"]: json.loads(l) for l in open("../corpus.mainnet.jsonl")}
b = H.Backend("localhost:19000", secure=False, timeout=120)

for cid in ["tx.sender_not_move_call.anchored.shared",
            "tx.degenerate.dense_and_dense.empty.archival"]:
    r = recs[cid]
    print(f"=== {cid} ({r['rpc']}) ===")
    for trial in range(3):
        req = H.base_request(r, identity_mask=True)
        dr = H.drain(b.send_fn(r["rpc"]), r["rpc"], req, full=True, max_drain=80000)
        err = getattr(dr, "error", None)
        if err:
            print(f"  trial {trial}: ERROR -> {str(err)[:90]}")
        else:
            end = getattr(dr, "end_reason", getattr(dr, "end", "?"))
            print(f"  trial {trial}: ok items={len(dr.ids)} capped={dr.capped} end={end}")
