#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Benchmark the archival sender-and-Move-call intersection query.

Each measured iteration starts at the original checkpoint boundary and follows
stable-v2 watermark cursors until it receives the expected transaction. The
reported end-to-end latency therefore includes every continuation RPC required
to find the transaction. One gRPC channel and stub are reused for all warmup
and measured iterations; the connection is never closed between requests.
"""

import argparse
import json
import math
import os
import statistics
import sys
import time
from collections import Counter
from dataclasses import dataclass

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))

import grpc  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2 as ledger  # noqa: E402
from sui.rpc.v2 import ledger_service_pb2_grpc as ledger_grpc  # noqa: E402
from sui.rpc.v2 import query_options_pb2 as query_options  # noqa: E402

START_CHECKPOINT = 293_472_171
END_CHECKPOINT = 298_403_339
SENDER = "0xf5a0ab9c8a632df614912d82de5d75b7b7649353c4dd29e8fd20f8aef35b976a"
MOVE_CALL = "0x0000000000000000000000000000000000000000000000000000000000000002::coin::zero"
EXPECTED_DIGEST = "ENuDcvWtHVuCXDqR3yx9EEZq4pqrrKBey1gJaDu7vzQJ"
RESUMABLE_END_REASONS = {
    query_options.QUERY_END_REASON_ITEM_LIMIT,
    query_options.QUERY_END_REASON_SCAN_LIMIT,
}


class ProtocolError(RuntimeError):
    """The server response cannot be resumed or does not match the query oracle."""


@dataclass
class QueryRun:
    latency_ms: float
    page_latencies_ms: list[float]
    continuations: int
    scan_limit_pages: int
    final_end_reason: int


def build_request() -> ledger.ListTransactionsRequest:
    request = ledger.ListTransactionsRequest(
        start_checkpoint=START_CHECKPOINT,
        end_checkpoint=END_CHECKPOINT,
    )
    request.read_mask.paths.append("digest")
    term = request.filter.terms.add()
    term.literals.add().sender.address = SENDER
    term.literals.add().move_call.function = MOVE_CALL
    request.options.limit = 1
    request.options.ordering = query_options.ORDERING_ASCENDING
    return request


def run_query(stub, base_request, *, timeout: float, max_continuations: int) -> QueryRun:
    query_started = time.perf_counter()
    page_latencies_ms = []
    resume_cursor = None
    continuations = 0
    scan_limit_pages = 0

    while True:
        request = type(base_request)()
        request.CopyFrom(base_request)
        if resume_cursor is not None:
            request.options.after = resume_cursor

        page_started = time.perf_counter()
        page_start_cursor = resume_cursor
        final_cursor = None
        end_reason = None
        payload_digests = []
        end_frames = 0

        for response in stub.ListTransactions(request, timeout=timeout):
            if not response.HasField("watermark"):
                raise ProtocolError("response frame missing required watermark")
            if not response.watermark.HasField("cursor") or not response.watermark.cursor:
                raise ProtocolError("watermark missing required cursor")
            final_cursor = response.watermark.cursor

            if response.HasField("transaction"):
                if not response.transaction.digest:
                    raise ProtocolError("transaction frame missing digest")
                payload_digests.append(response.transaction.digest)

            if response.HasField("end"):
                end_frames += 1
                end_reason = response.end.reason

        page_latencies_ms.append((time.perf_counter() - page_started) * 1000)

        if end_frames != 1:
            raise ProtocolError(f"successful stream contained {end_frames} QueryEnd frames")
        if final_cursor is None:
            raise ProtocolError("successful stream contained no watermark cursor")
        if len(payload_digests) > 1:
            raise ProtocolError(f"limit=1 stream returned {len(payload_digests)} transactions")

        if payload_digests:
            digest = payload_digests[0]
            if digest != EXPECTED_DIGEST:
                raise ProtocolError(
                    f"query returned unexpected digest {digest}; expected {EXPECTED_DIGEST}"
                )
            if end_reason != query_options.QUERY_END_REASON_ITEM_LIMIT:
                raise ProtocolError(
                    "matching transaction ended with "
                    f"{query_options.QueryEndReason.Name(end_reason)}, expected "
                    "QUERY_END_REASON_ITEM_LIMIT"
                )
            return QueryRun(
                latency_ms=(time.perf_counter() - query_started) * 1000,
                page_latencies_ms=page_latencies_ms,
                continuations=continuations,
                scan_limit_pages=scan_limit_pages,
                final_end_reason=end_reason,
            )

        if end_reason not in RESUMABLE_END_REASONS:
            raise ProtocolError(
                "query ended before finding the expected transaction: "
                f"{query_options.QueryEndReason.Name(end_reason)}"
            )
        if final_cursor == page_start_cursor:
            raise ProtocolError("resumable QueryEnd did not advance watermark cursor")
        if end_reason == query_options.QUERY_END_REASON_SCAN_LIMIT:
            scan_limit_pages += 1
        if continuations >= max_continuations:
            raise ProtocolError(
                f"query exceeded the {max_continuations} continuation safety limit"
            )

        resume_cursor = final_cursor
        continuations += 1


def percentile(values: list[float], quantile: float) -> float:
    if not values:
        raise ValueError("cannot summarize an empty sample")
    ordered = sorted(values)
    rank = (len(ordered) - 1) * quantile
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (rank - lower)


def distribution(values: list[float], *, digits: int = 3) -> dict:
    return {
        "min": round(min(values), digits),
        "p50": round(percentile(values, 0.50), digits),
        "p90": round(percentile(values, 0.90), digits),
        "p95": round(percentile(values, 0.95), digits),
        "p99": round(percentile(values, 0.99), digits),
        "mean": round(statistics.fmean(values), digits),
        "max": round(max(values), digits),
    }


def histogram(values: list[int]) -> dict[str, int]:
    return {str(value): count for value, count in sorted(Counter(values).items())}


def create_channel(args):
    options = [("grpc.max_receive_message_length", 128 * 1024 * 1024)]
    if args.server_name:
        options.append(("grpc.ssl_target_name_override", args.server_name))

    if args.tls:
        root_certificates = None
        if args.ca:
            with open(args.ca, "rb") as certificate_file:
                root_certificates = certificate_file.read()
        credentials = grpc.ssl_channel_credentials(root_certificates=root_certificates)
        return grpc.secure_channel(args.target, credentials, options=options)
    return grpc.insecure_channel(args.target, options=options)


def parse_args():
    parser = argparse.ArgumentParser(
        description="benchmark the exact archival sender-and-Move-call ListTransactions query"
    )
    parser.add_argument(
        "--target",
        default=os.environ.get("HOST", "localhost:8000"),
        help="LedgerService host:port (default: HOST or localhost:8000)",
    )
    parser.add_argument("--iterations", type=int, default=1000)
    parser.add_argument("--warmup", type=int, default=10)
    parser.add_argument("--timeout", type=float, default=60, help="per-RPC deadline in seconds")
    parser.add_argument("--connect-timeout", type=float, default=30)
    parser.add_argument("--max-continuations", type=int, default=100)
    parser.add_argument("--progress-every", type=int, default=100)
    parser.add_argument("--tls", action="store_true")
    parser.add_argument("--ca", help="PEM root certificate for TLS")
    parser.add_argument("--server-name", help="TLS authority/SAN override")
    args = parser.parse_args()
    if args.iterations <= 0:
        parser.error("--iterations must be positive")
    if args.warmup < 0:
        parser.error("--warmup cannot be negative")
    if args.timeout <= 0 or args.connect_timeout <= 0:
        parser.error("timeouts must be positive")
    if args.max_continuations < 0:
        parser.error("--max-continuations cannot be negative")
    if args.progress_every < 0:
        parser.error("--progress-every cannot be negative")
    if (args.ca or args.server_name) and not args.tls:
        parser.error("--ca and --server-name require --tls")
    return args


def main() -> int:
    args = parse_args()
    request = build_request()
    channel = create_channel(args)
    try:
        grpc.channel_ready_future(channel).result(timeout=args.connect_timeout)
        stub = ledger_grpc.LedgerServiceStub(channel)

        for warmup_index in range(args.warmup):
            run_query(
                stub,
                request,
                timeout=args.timeout,
                max_continuations=args.max_continuations,
            )
            if args.progress_every and (warmup_index + 1) % args.progress_every == 0:
                print(f"warmup {warmup_index + 1}/{args.warmup}", file=sys.stderr, flush=True)

        runs = []
        benchmark_started = time.perf_counter()
        for iteration in range(args.iterations):
            runs.append(
                run_query(
                    stub,
                    request,
                    timeout=args.timeout,
                    max_continuations=args.max_continuations,
                )
            )
            if args.progress_every and (iteration + 1) % args.progress_every == 0:
                print(
                    f"measured {iteration + 1}/{args.iterations}",
                    file=sys.stderr,
                    flush=True,
                )
        elapsed_seconds = time.perf_counter() - benchmark_started
    except (grpc.RpcError, grpc.FutureTimeoutError, ProtocolError) as error:
        print(f"benchmark failed: {error}", file=sys.stderr)
        return 1
    finally:
        channel.close()

    end_to_end_latencies = [run.latency_ms for run in runs]
    page_latencies = [latency for run in runs for latency in run.page_latencies_ms]
    continuation_counts = [run.continuations for run in runs]
    scan_limit_counts = [run.scan_limit_pages for run in runs]
    summary = {
        "target": args.target,
        "connection": "single_reused_grpc_channel",
        "transport": "tls" if args.tls else "plaintext",
        "iterations": args.iterations,
        "warmup_iterations": args.warmup,
        "elapsed_seconds": round(elapsed_seconds, 3),
        "query": {
            "start_checkpoint": START_CHECKPOINT,
            "end_checkpoint": END_CHECKPOINT,
            "ordering": "ORDERING_ASCENDING",
            "limit": 1,
            "sender": SENDER,
            "move_call": MOVE_CALL,
            "expected_digest": EXPECTED_DIGEST,
        },
        "end_to_end_latency_ms": distribution(end_to_end_latencies),
        "rpc_page_latency_ms": distribution(page_latencies),
        "continuations": {
            **distribution(continuation_counts),
            "histogram": histogram(continuation_counts),
        },
        "scan_limit_pages": {
            "total": sum(scan_limit_counts),
            "histogram_per_iteration": histogram(scan_limit_counts),
        },
        "rpc_pages": len(page_latencies),
    }
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
