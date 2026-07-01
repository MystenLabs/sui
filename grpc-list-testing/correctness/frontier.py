#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Probe a node's GetServiceInfo (sui.rpc.v2.LedgerService) to discover the
checkpoint frontier, so the corpus generator can clamp CP_CEILING to a bound the
backend actually serves.

  python frontier.py --target localhost:18000 --tls --ca kvrpc.testnet.crt \
      --server-name kv-rpc-http2.rpc-kv-testnet.svc.cluster.local

`checkpoint_height` is the most-recently-executed checkpoint the node reports.
NOTE: on a kv-rpc this is the RAW BigTable tip, which can lead the *bitmap-index*
frontier that filtered List queries actually serve. For a safe corpus ceiling,
probe a FULLNODE (reports the live network tip) and subtract a margin, or take the
min across the fullnode tip and the kv-rpc index frontier.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import grpc  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2 as v2  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2_grpc as v2grpc  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True)
    ap.add_argument("--tls", action="store_true")
    ap.add_argument("--ca")
    ap.add_argument("--server-name")
    ap.add_argument("--margin", type=int, default=0,
                    help="subtract from checkpoint_height to print a suggested --ceiling")
    args = ap.parse_args()

    opts = [("grpc.max_receive_message_length", 64 * 1024 * 1024)]
    if args.tls:
        ca = open(args.ca, "rb").read() if args.ca else None
        creds = grpc.ssl_channel_credentials(root_certificates=ca)
        if args.server_name:
            opts.append(("grpc.ssl_target_name_override", args.server_name))
        ch = grpc.secure_channel(args.target, creds, options=opts)
    else:
        ch = grpc.insecure_channel(args.target, options=opts)

    info = v2grpc.LedgerServiceStub(ch).GetServiceInfo(v2.GetServiceInfoRequest(), timeout=30)
    print(f"chain_id            : {info.chain_id}")
    print(f"epoch               : {info.epoch}")
    print(f"checkpoint_height   : {info.checkpoint_height}")
    print(f"lowest_available_cp : {info.lowest_available_checkpoint}")
    print(f"server              : {info.server}")
    if args.margin:
        print(f"suggested --ceiling : {info.checkpoint_height - args.margin}")


if __name__ == "__main__":
    main()
