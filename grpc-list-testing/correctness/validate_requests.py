# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Validate corpus and load JSONL requests against stable-v2 protobuf types."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from google.protobuf import json_format

HERE = Path(__file__).resolve().parent
STUBS = HERE / "sui_pb"
if str(STUBS) not in sys.path:
    sys.path.insert(0, str(STUBS))

from sui.rpc.v2 import checkpoint_pb2  # noqa: E402
from sui.rpc.v2 import event_pb2  # noqa: E402
from sui.rpc.v2 import executed_transaction_pb2  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2  # noqa: E402

REQUEST_TYPES = {
    "ListTransactions": ledger_service_pb2.ListTransactionsRequest,
    "ListEvents": ledger_service_pb2.ListEventsRequest,
    "ListCheckpoints": ledger_service_pb2.ListCheckpointsRequest,
}
READ_MASK_TARGETS = {
    "ListTransactions": executed_transaction_pb2.ExecutedTransaction.DESCRIPTOR,
    "ListEvents": event_pb2.Event.DESCRIPTOR,
    "ListCheckpoints": checkpoint_pb2.Checkpoint.DESCRIPTOR,
}


def validate_file(path: Path) -> bool:
    parsed = 0
    records = 0
    failed = False
    try:
        lines = path.read_text().splitlines()
    except OSError as error:
        print(f"{path}: {error}", file=sys.stderr)
        return False

    for line_number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        records += 1
        try:
            record = json.loads(line)
            if not isinstance(record, dict):
                raise ValueError("record must be a JSON object")
            rpc = record.get("rpc")
            if rpc is None:
                raise ValueError("missing rpc")
            request_type = REQUEST_TYPES.get(rpc)
            if request_type is None:
                raise ValueError(f"unknown rpc {rpc!r}")
            if "request" not in record:
                raise ValueError("missing request")
            request = json_format.ParseDict(record["request"], request_type())
            if request.HasField("read_mask") and not request.read_mask.IsValidForDescriptor(
                READ_MASK_TARGETS[rpc]
            ):
                raise ValueError(
                    f"invalid {rpc} read_mask {request.read_mask.ToJsonString()!r}"
                )
        except (json.JSONDecodeError, json_format.ParseError, TypeError, ValueError) as error:
            print(f"{path}:{line_number}: {error}", file=sys.stderr)
            failed = True
            continue
        parsed += 1

    print(f"{path}: parsed {parsed}/{records} requests")
    return not failed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", type=Path, help="corpus or load JSONL files")
    args = parser.parse_args()
    results = [validate_file(path) for path in args.paths]
    return 0 if all(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
