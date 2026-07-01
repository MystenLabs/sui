import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc
from sui.rpc.v2 import ledger_service_pb2 as lv2
from sui.rpc.v2 import ledger_service_pb2_grpc as lv2g

stub = lv2g.LedgerServiceStub(grpc.insecure_channel("localhost:19000"))
info = stub.GetServiceInfo(lv2.GetServiceInfoRequest(), timeout=30)
tip = info.checkpoint_height
lo = info.lowest_available_checkpoint
print(f"chain_id: {info.chain_id}")
print(f"epoch: {info.epoch}")
print(f"checkpoint_height (tip): {tip:,}")
print(f"lowest_available_checkpoint: {lo:,}")
print(f"lowest_available_checkpoint_objects: {info.lowest_available_checkpoint_objects:,}")
print(f"served window: [{lo:,}, {tip:,}] = {tip-lo:,} checkpoints")
print(f"server: {info.server}")
