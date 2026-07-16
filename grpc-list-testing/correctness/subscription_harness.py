#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Record stable-v2 subscriptions and verify them against Snowflake."""

import argparse
import json
import os
import queue
import re
import subprocess
import sys
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))

from google.protobuf import json_format  # noqa: E402
from sui.rpc.v2 import subscription_service_pb2 as ss  # noqa: E402
from sui.rpc.v2 import subscription_service_pb2_grpc as ss_grpc  # noqa: E402


SCHEMA_VERSION = 1
DEFAULT_CASES_PATH = Path(__file__).with_name("subscription_cases.testnet.jsonl")
SUPPORTED_RPCS = (
    "SubscribeCheckpoints",
    "SubscribeTransactions",
    "SubscribeEvents",
)
REQUEST_TYPES = {
    "SubscribeCheckpoints": ss.SubscribeCheckpointsRequest,
    "SubscribeTransactions": ss.SubscribeTransactionsRequest,
    "SubscribeEvents": ss.SubscribeEventsRequest,
}
RESPONSE_TYPES = {
    "SubscribeCheckpoints": ss.SubscribeCheckpointsResponse,
    "SubscribeTransactions": ss.SubscribeTransactionsResponse,
    "SubscribeEvents": ss.SubscribeEventsResponse,
}
PAYLOAD_FIELDS = {
    "SubscribeCheckpoints": "checkpoint",
    "SubscribeTransactions": "transaction",
    "SubscribeEvents": "event",
}
SCHEMA_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


class FixtureError(ValueError):
    pass


class CaptureFormatError(ValueError):
    pass


class SnowflakeError(RuntimeError):
    pass


@dataclass(frozen=True)
class SubscriptionCase:
    id: str
    rpc: str
    request: object

    @property
    def filtered(self) -> bool:
        return self.request.HasField("filter")

    def as_dict(self) -> dict:
        return {
            "id": self.id,
            "rpc": self.rpc,
            "request": message_to_dict(self.request),
        }


@dataclass
class FrameState:
    frames: int = 0
    payloads: int = 0
    ready: bool = False
    last_cursor: int | None = None
    last_payload_cursor: int | None = None
    last_payload_position: tuple | None = None
    final_covered_checkpoint: int | None = None
    payload_identities: set = field(default_factory=set)


@dataclass(frozen=True)
class SnowflakeConfig:
    connection: str = "nick"
    warehouse: str = "ANALYTICS_WH"
    schema: str = "CHAINDATA_TESTNET"
    warehouse_wait_seconds: int = 1800
    cases_path: Path = DEFAULT_CASES_PATH


@dataclass
class ParsedCapture:
    header: dict
    summary: dict
    cases: list[SubscriptionCase]
    frames: list[tuple[SubscriptionCase, object]]
    structural_reasons: dict[str, list[str]]


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def message_to_dict(message) -> dict:
    return json_format.MessageToDict(message, preserving_proto_field_name=True)


def write_json_line(output, record: dict) -> None:
    output.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
    output.flush()


def _reject_quote(value: str, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise FixtureError(f"{label} must be a non-empty string")
    if "'" in value:
        raise FixtureError(f"{label} must not contain a quote")
    return value.replace("'", "''")


def _split_move_path(value: str, label: str) -> list[str]:
    _reject_quote(value, label)
    parts = []
    start = 0
    depth = 0
    index = 0
    while index < len(value):
        char = value[index]
        if char == "<":
            depth += 1
        elif char == ">":
            depth -= 1
            if depth < 0:
                raise FixtureError(f"{label} has unbalanced generic brackets")
        elif char == ":" and index + 1 < len(value) and value[index + 1] == ":" and depth == 0:
            parts.append(value[start:index])
            index += 1
            start = index + 1
        index += 1
    if depth:
        raise FixtureError(f"{label} has unbalanced generic brackets")
    parts.append(value[start:])
    if any(not part for part in parts):
        raise FixtureError(f"{label} has an empty path component")
    return parts


def _literal_sql(case: SubscriptionCase, literal, table_alias: str) -> str:
    predicate_name = literal.WhichOneof("predicate")
    if case.rpc in ("SubscribeCheckpoints", "SubscribeTransactions"):
        if predicate_name != "sender":
            raise FixtureError(f"{case.id}: unsupported transaction predicate {predicate_name!r}")
        address = _reject_quote(literal.sender.address, f"{case.id} sender.address")
        predicate = f"{table_alias}.SENDER = '{address}'"
    elif case.rpc == "SubscribeEvents":
        if predicate_name == "sender":
            address = _reject_quote(literal.sender.address, f"{case.id} sender.address")
            predicate = f"{table_alias}.SENDER = '{address}'"
        elif predicate_name == "emit_module":
            value = literal.emit_module.module
            parts = _split_move_path(value, f"{case.id} emit_module.module")
            if len(parts) != 2:
                raise FixtureError(f"{case.id}: emit_module.module must contain package and module")
            package, module = (_reject_quote(part, f"{case.id} emit_module.module") for part in parts)
            predicate = f"{table_alias}.PACKAGE = '{package}' AND {table_alias}.MODULE = '{module}'"
        elif predicate_name == "event_type":
            value = literal.event_type.event_type
            parts = _split_move_path(value, f"{case.id} event_type.event_type")
            escaped = _reject_quote(value, f"{case.id} event_type.event_type")
            if len(parts) in (1, 2):
                predicate = f"STARTSWITH({table_alias}.EVENT_TYPE, '{escaped}::')"
            elif len(parts) == 3 and "<" not in parts[2]:
                predicate = (
                    f"({table_alias}.EVENT_TYPE = '{escaped}' OR "
                    f"STARTSWITH({table_alias}.EVENT_TYPE, '{escaped}<'))"
                )
            elif len(parts) == 3 and parts[2].count("<") == 1 and parts[2].endswith(">"):
                predicate = f"{table_alias}.EVENT_TYPE = '{escaped}'"
            else:
                raise FixtureError(f"{case.id}: malformed event_type.event_type")
        else:
            raise FixtureError(f"{case.id}: unsupported event predicate {predicate_name!r}")
    else:
        raise FixtureError(f"{case.id}: unsupported rpc {case.rpc!r}")

    expression = f"COALESCE(({predicate}), FALSE)"
    return f"NOT ({expression})" if literal.negated else expression


def compile_filter_sql(case: SubscriptionCase, table_alias: str) -> str:
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", table_alias):
        raise FixtureError(f"invalid SQL table alias {table_alias!r}")
    if not case.filtered:
        return "TRUE"
    if not case.request.filter.terms:
        raise FixtureError(f"{case.id}: filter must contain at least one term")

    terms = []
    for term in case.request.filter.terms:
        if not term.literals:
            raise FixtureError(f"{case.id}: filter terms must contain at least one literal")
        literals = [_literal_sql(case, literal, table_alias) for literal in term.literals]
        terms.append(f"({' AND '.join(literals)})")
    return f"({' OR '.join(terms)})"


def load_cases(path: Path) -> list[SubscriptionCase]:
    cases = []
    seen_ids = set()
    try:
        lines = path.read_text().splitlines()
    except OSError as error:
        raise FixtureError(f"unable to read cases {path}: {error}") from error

    for line_number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise FixtureError(f"{path}:{line_number}: invalid JSON: {error}") from error
        if not isinstance(record, dict):
            raise FixtureError(f"{path}:{line_number}: record must be an object")
        keys = set(record)
        required = {"id", "rpc", "request"}
        if keys != required:
            unknown = sorted(keys - required)
            missing = sorted(required - keys)
            raise FixtureError(
                f"{path}:{line_number}: keys must be exactly {sorted(required)}; "
                f"unknown={unknown}, missing={missing}"
            )
        case_id = record["id"]
        rpc = record["rpc"]
        if not isinstance(case_id, str) or not case_id:
            raise FixtureError(f"{path}:{line_number}: id must be a non-empty string")
        if case_id in seen_ids:
            raise FixtureError(f"{path}:{line_number}: duplicate id {case_id!r}")
        if rpc not in REQUEST_TYPES:
            raise FixtureError(f"{path}:{line_number}: unsupported rpc {rpc!r}")
        if not isinstance(record["request"], dict):
            raise FixtureError(f"{path}:{line_number}: request must be an object")
        try:
            request = json_format.ParseDict(record["request"], REQUEST_TYPES[rpc]())
        except (json_format.ParseError, TypeError, ValueError) as error:
            raise FixtureError(f"{path}:{line_number}: invalid {rpc} request: {error}") from error
        case = SubscriptionCase(case_id, rpc, request)
        compile_filter_sql(case, "t" if rpc != "SubscribeEvents" else "e")
        cases.append(case)
        seen_ids.add(case_id)

    if not cases:
        raise FixtureError(f"{path}: no subscription cases")
    return cases


def payload_for(case: SubscriptionCase, response):
    field_name = PAYLOAD_FIELDS[case.rpc]
    return getattr(response, field_name) if response.HasField(field_name) else None


def payload_identity(case: SubscriptionCase, payload):
    if case.rpc == "SubscribeCheckpoints":
        return payload.sequence_number
    if case.rpc == "SubscribeTransactions":
        return (payload.digest, payload.checkpoint)
    if case.rpc == "SubscribeEvents":
        return (payload.transaction_digest, payload.event_index, payload.checkpoint)
    raise ValueError(case.rpc)


def _validate_payload_identity(case: SubscriptionCase, payload) -> tuple[list[str], tuple | int | None, tuple | None]:
    reasons = []
    identity = None
    position = None
    if case.rpc == "SubscribeCheckpoints":
        if not payload.HasField("sequence_number"):
            reasons.append("checkpoint payload is missing sequence_number")
        if not payload.HasField("digest") or not payload.digest:
            reasons.append("checkpoint payload is missing digest")
        if not reasons:
            identity = payload.sequence_number
    elif case.rpc == "SubscribeTransactions":
        if not payload.HasField("digest") or not payload.digest:
            reasons.append("transaction payload is missing digest")
        if not payload.HasField("checkpoint"):
            reasons.append("transaction payload is missing checkpoint")
        if not payload.HasField("transaction_index"):
            reasons.append("transaction payload is missing transaction_index")
        if not reasons:
            identity = (payload.digest, payload.checkpoint)
            position = (payload.checkpoint, payload.transaction_index)
    elif case.rpc == "SubscribeEvents":
        if not payload.HasField("transaction_digest") or not payload.transaction_digest:
            reasons.append("event payload is missing transaction_digest")
        if not payload.HasField("event_index"):
            reasons.append("event payload is missing event_index")
        if not payload.HasField("checkpoint"):
            reasons.append("event payload is missing checkpoint")
        if not payload.HasField("transaction_index"):
            reasons.append("event payload is missing transaction_index")
        if not reasons:
            identity = (payload.transaction_digest, payload.event_index, payload.checkpoint)
            position = (payload.checkpoint, payload.transaction_index, payload.event_index)
    return reasons, identity, position


def validate_frame(case: SubscriptionCase, response, state: FrameState) -> list[str]:
    reasons = []
    first_frame = state.frames == 0
    state.frames += 1
    payload = payload_for(case, response)

    if case.rpc == "SubscribeCheckpoints":
        if not response.HasField("cursor"):
            reasons.append("checkpoint frame is missing cursor")
            cursor = None
        else:
            cursor = response.cursor
            if state.last_cursor is not None and cursor < state.last_cursor:
                reasons.append(f"checkpoint cursor regressed from {state.last_cursor} to {cursor}")
        if first_frame and case.filtered:
            if payload is not None:
                reasons.append("filtered checkpoint stream did not start with a progress-only frame")
        if not case.filtered and payload is None:
            reasons.append("unfiltered checkpoint frame is missing its checkpoint payload")

        if payload is not None:
            payload_reasons, identity, _ = _validate_payload_identity(case, payload)
            reasons.extend(payload_reasons)
            if cursor is not None and payload.HasField("sequence_number") and payload.sequence_number != cursor:
                reasons.append(
                    f"checkpoint payload sequence_number {payload.sequence_number} does not equal cursor {cursor}"
                )
            if (
                not case.filtered
                and state.last_payload_cursor is not None
                and cursor is not None
                and cursor != state.last_payload_cursor + 1
            ):
                reasons.append(
                    f"unfiltered checkpoint payload cursor {cursor} did not follow {state.last_payload_cursor}"
                )
            if identity is not None:
                if identity in state.payload_identities:
                    reasons.append(f"duplicate payload identity {identity!r}")
                else:
                    state.payload_identities.add(identity)
                state.payloads += 1
            if cursor is not None:
                state.last_payload_cursor = cursor
        if cursor is not None:
            state.last_cursor = cursor
            state.final_covered_checkpoint = cursor
    else:
        if not response.HasField("watermark"):
            reasons.append(f"{case.rpc} frame is missing watermark")
            watermark = None
        else:
            watermark = response.watermark
            if not watermark.HasField("cursor") or not watermark.cursor:
                reasons.append(f"{case.rpc} watermark is missing cursor")
            if watermark.HasField("checkpoint"):
                covered = watermark.checkpoint
                if (
                    state.final_covered_checkpoint is not None
                    and covered < state.final_covered_checkpoint
                ):
                    reasons.append(
                        f"covered checkpoint regressed from {state.final_covered_checkpoint} to {covered}"
                    )
                state.final_covered_checkpoint = covered
        if first_frame and case.filtered:
            if payload is not None:
                reasons.append(f"filtered {case.rpc} stream did not start with a progress-only frame")
            if watermark is not None and watermark.HasField("checkpoint"):
                reasons.append(f"filtered {case.rpc} first watermark unexpectedly covered a checkpoint")

        if payload is not None:
            payload_reasons, identity, position = _validate_payload_identity(case, payload)
            reasons.extend(payload_reasons)
            if position is not None and state.last_payload_position is not None and position <= state.last_payload_position:
                reasons.append(
                    f"payload position {position!r} did not follow {state.last_payload_position!r}"
                )
            if identity is not None:
                if identity in state.payload_identities:
                    reasons.append(f"duplicate payload identity {identity!r}")
                else:
                    state.payload_identities.add(identity)
                state.payloads += 1
            if position is not None:
                state.last_payload_position = position

    if first_frame and not reasons:
        state.ready = True
    return reasons


class SubscriptionBackend:
    def __init__(self, target: str, tls: bool = False, ca_path: Path | None = None,
                 server_name: str | None = None, timeout: int = 900):
        import grpc

        options = [("grpc.max_receive_message_length", 512 * 1024 * 1024)]
        if tls:
            try:
                root_certificates = ca_path.read_bytes() if ca_path else None
            except OSError as error:
                raise FixtureError(f"unable to read CA {ca_path}: {error}") from error
            credentials = grpc.ssl_channel_credentials(root_certificates=root_certificates)
            if server_name:
                options.append(("grpc.ssl_target_name_override", server_name))
            self.channel = grpc.secure_channel(target, credentials, options=options)
        else:
            self.channel = grpc.insecure_channel(target, options=options)
        self.stub = ss_grpc.SubscriptionServiceStub(self.channel)
        self.timeout = timeout
        self.methods = {rpc: getattr(self.stub, rpc) for rpc in SUPPORTED_RPCS}

    def open(self, case: SubscriptionCase):
        return self.methods[case.rpc](case.request, timeout=self.timeout)

    def close(self) -> None:
        self.channel.close()


def _stream_worker(case: SubscriptionCase, backend: SubscriptionBackend, events: queue.Queue) -> None:
    try:
        call = backend.open(case)
        events.put(("call", case.id, call))
        for response in call:
            events.put(("frame", case.id, utc_now(), response))
        events.put(("eof", case.id))
    except BaseException as error:
        events.put(("error", case.id, error))


def _grpc_error_name(error: BaseException) -> str:
    try:
        code = error.code()
        return getattr(code, "name", str(code))
    except Exception:
        return type(error).__name__


def default_capture_path() -> Path:
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path(f"/tmp/sui-subscription-{timestamp}-{uuid.uuid4().hex[:8]}.jsonl")


def record_capture(
    cases: list[SubscriptionCase],
    target: str,
    output_path: Path,
    *,
    tls: bool = False,
    ca_path: Path | None = None,
    server_name: str | None = None,
    rpc_deadline_seconds: int = 900,
    observation_checkpoints: int = 100,
    start_timeout_seconds: int = 120,
    capture_timeout_seconds: int = 600,
    backend_factory=SubscriptionBackend,
) -> int:
    if observation_checkpoints <= 0:
        raise FixtureError("observation_checkpoints must be positive")
    if min(rpc_deadline_seconds, start_timeout_seconds, capture_timeout_seconds) <= 0:
        raise FixtureError("all timeout values must be positive")

    output_path = output_path.expanduser().resolve()
    print(f"capture={output_path}", flush=True)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    run_id = uuid.uuid4().hex
    started_at = utc_now()
    states = {case.id: FrameState() for case in cases}
    cases_by_id = {case.id: case for case in cases}
    header = {
        "type": "header",
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "started_at": started_at,
        "target": target,
        "cases": [case.as_dict() for case in cases],
    }

    backend = None
    try:
        with output_path.open("x", encoding="utf-8") as output:
            write_json_line(output, header)
            backend = backend_factory(
                target,
                tls=tls,
                ca_path=ca_path,
                server_name=server_name,
                timeout=rpc_deadline_seconds,
            )
            events = queue.Queue()
            calls = {}
            capture_errors = []
            receive_sequence = 0
            window_start = None
            window_end = None
            deliberate_cancellation = False
            stopping = False
            started_monotonic = time.monotonic()

            executor = ThreadPoolExecutor(max_workers=len(cases), thread_name_prefix="subscription")
            futures = [executor.submit(_stream_worker, case, backend, events) for case in cases]

            def stop_readers(deliberate: bool) -> None:
                nonlocal stopping, deliberate_cancellation
                if stopping:
                    return
                stopping = True
                deliberate_cancellation = deliberate
                for call in calls.values():
                    call.cancel()

            while True:
                all_workers_done = all(future.done() for future in futures)
                if stopping and all_workers_done and events.empty():
                    break

                elapsed = time.monotonic() - started_monotonic
                if not stopping and window_start is None and elapsed >= start_timeout_seconds:
                    capture_errors.append(f"start timeout after {start_timeout_seconds} seconds")
                    stop_readers(False)
                elif not stopping and elapsed >= capture_timeout_seconds:
                    capture_errors.append(f"capture timeout after {capture_timeout_seconds} seconds")
                    stop_readers(False)

                try:
                    event = events.get(timeout=0.1)
                except queue.Empty:
                    continue

                kind, case_id, *details = event
                case = cases_by_id[case_id]
                if kind == "call":
                    call = details[0]
                    calls[case_id] = call
                    if stopping:
                        call.cancel()
                    continue

                if kind == "frame":
                    received_at, response = details
                    receive_sequence += 1
                    all_ready_before_frame = all(state.ready for state in states.values())
                    reasons = validate_frame(case, response, states[case_id])
                    write_json_line(
                        output,
                        {
                            "type": "frame",
                            "case_id": case_id,
                            "rpc": case.rpc,
                            "receive_sequence": receive_sequence,
                            "received_at": received_at,
                            "response": message_to_dict(response),
                        },
                    )
                    if reasons and not stopping:
                        capture_errors.extend(
                            f"receive_sequence {receive_sequence} {case_id}: {reason}" for reason in reasons
                        )
                        stop_readers(False)
                        continue

                    payload = payload_for(case, response)
                    if (
                        not stopping
                        and window_start is None
                        and all_ready_before_frame
                        and case.id == "cp.unfiltered"
                        and payload is not None
                    ):
                        window_start = payload.sequence_number + 1
                        window_end = window_start + observation_checkpoints - 1
                        print(f"window={window_start}..{window_end}", flush=True)

                    if window_end is not None and not stopping:
                        complete = all(
                            state.final_covered_checkpoint is not None
                            and state.final_covered_checkpoint >= window_end
                            for state in states.values()
                        )
                        if complete:
                            stop_readers(True)
                    continue

                if kind == "eof":
                    if not stopping:
                        capture_errors.append(f"{case_id}: stream ended before common coverage")
                        stop_readers(False)
                    continue

                error = details[0]
                error_name = _grpc_error_name(error)
                if deliberate_cancellation and error_name == "CANCELLED":
                    continue
                if stopping and error_name == "CANCELLED":
                    continue
                if not stopping:
                    capture_errors.append(f"{case_id}: gRPC {error_name}: {error}")
                    stop_readers(False)

            executor.shutdown(wait=True)
            summary = {
                "type": "summary",
                "schema_version": SCHEMA_VERSION,
                "run_id": run_id,
                "finished_at": utc_now(),
                "window_start": window_start,
                "window_end": window_end,
                "deliberate_cancellation": deliberate_cancellation,
                "capture_errors": capture_errors,
                "case_summaries": [
                    {
                        "case_id": case.id,
                        "rpc": case.rpc,
                        "frame_count": states[case.id].frames,
                        "payload_count": states[case.id].payloads,
                        "final_covered_checkpoint": states[case.id].final_covered_checkpoint,
                    }
                    for case in cases
                ],
            }
            write_json_line(output, summary)
    except FileExistsError as error:
        raise FixtureError(f"capture output already exists: {output_path}") from error
    finally:
        if backend is not None:
            backend.close()

    if capture_errors:
        print(f"capture failed: {len(capture_errors)} error(s)", file=sys.stderr)
        return 1
    print(f"capture complete: {len(cases)} cases", flush=True)
    return 0


def parse_capture(capture_path: Path, cases: list[SubscriptionCase]) -> ParsedCapture:
    try:
        raw_lines = capture_path.read_text().splitlines()
    except OSError as error:
        raise CaptureFormatError(f"unable to read capture {capture_path}: {error}") from error
    if not raw_lines:
        raise CaptureFormatError("capture is empty")
    records = []
    for line_number, line in enumerate(raw_lines, 1):
        if not line.strip():
            raise CaptureFormatError(f"capture line {line_number} is blank")
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise CaptureFormatError(f"capture line {line_number} is invalid JSON: {error}") from error
        if not isinstance(record, dict):
            raise CaptureFormatError(f"capture line {line_number} is not an object")
        records.append(record)

    headers = [record for record in records if record.get("type") == "header"]
    summaries = [record for record in records if record.get("type") == "summary"]
    if len(headers) != 1 or records[0] is not headers[0]:
        raise CaptureFormatError("capture must have exactly one header as its first record")
    if len(summaries) != 1 or records[-1] is not summaries[0]:
        raise CaptureFormatError("capture must have exactly one summary as its final record")
    if any(record.get("type") not in {"header", "frame", "summary"} for record in records):
        raise CaptureFormatError("capture contains an unknown record type")

    header = headers[0]
    summary = summaries[0]
    if header.get("schema_version") != SCHEMA_VERSION or summary.get("schema_version") != SCHEMA_VERSION:
        raise CaptureFormatError("unsupported capture schema_version")
    if header.get("run_id") != summary.get("run_id"):
        raise CaptureFormatError("header and summary run_id differ")
    expected_case_records = [case.as_dict() for case in cases]
    if header.get("cases") != expected_case_records:
        raise CaptureFormatError("capture header does not match the cases fixture")
    if summary.get("capture_errors"):
        raise CaptureFormatError(f"capture summary reports errors: {summary['capture_errors']!r}")
    if not summary.get("deliberate_cancellation"):
        raise CaptureFormatError("capture did not end through deliberate cancellation")

    window_start = summary.get("window_start")
    window_end = summary.get("window_end")
    if not isinstance(window_start, int) or not isinstance(window_end, int) or window_end < window_start:
        raise CaptureFormatError("capture summary has an invalid observation window")

    cases_by_id = {case.id: case for case in cases}
    states = {case.id: FrameState() for case in cases}
    structural_reasons = {case.id: [] for case in cases}
    parsed_frames = []
    expected_sequence = 1
    for line_number, record in enumerate(records[1:-1], 2):
        if record.get("type") != "frame":
            raise CaptureFormatError(f"capture line {line_number} is not a frame")
        if record.get("receive_sequence") != expected_sequence:
            raise CaptureFormatError(
                f"capture receive_sequence is not contiguous at line {line_number}: "
                f"expected {expected_sequence}, got {record.get('receive_sequence')!r}"
            )
        expected_sequence += 1
        case_id = record.get("case_id")
        if case_id not in cases_by_id:
            raise CaptureFormatError(f"capture line {line_number} has unknown case_id {case_id!r}")
        case = cases_by_id[case_id]
        if record.get("rpc") != case.rpc:
            raise CaptureFormatError(f"capture line {line_number} rpc does not match case {case_id}")
        if not isinstance(record.get("response"), dict):
            raise CaptureFormatError(f"capture line {line_number} response is not an object")
        try:
            response = json_format.ParseDict(record["response"], RESPONSE_TYPES[case.rpc]())
        except (json_format.ParseError, TypeError, ValueError) as error:
            raise CaptureFormatError(f"capture line {line_number} has an invalid response: {error}") from error
        reasons = validate_frame(case, response, states[case_id])
        structural_reasons[case_id].extend(
            f"receive_sequence {record['receive_sequence']}: {reason}" for reason in reasons
        )
        parsed_frames.append((case, response))

    case_summaries = summary.get("case_summaries")
    if not isinstance(case_summaries, list):
        raise CaptureFormatError("capture summary is missing case_summaries")
    summaries_by_id = {}
    for case_summary in case_summaries:
        if not isinstance(case_summary, dict) or case_summary.get("case_id") not in cases_by_id:
            raise CaptureFormatError("capture summary has an invalid case summary")
        case_id = case_summary["case_id"]
        if case_id in summaries_by_id:
            raise CaptureFormatError(f"capture summary repeats case {case_id}")
        summaries_by_id[case_id] = case_summary
    if set(summaries_by_id) != set(cases_by_id):
        raise CaptureFormatError("capture summary does not cover every fixture case")

    for case in cases:
        state = states[case.id]
        case_summary = summaries_by_id[case.id]
        if case_summary.get("rpc") != case.rpc:
            raise CaptureFormatError(f"capture summary rpc does not match case {case.id}")
        if case_summary.get("frame_count") != state.frames:
            raise CaptureFormatError(f"capture summary frame_count does not match case {case.id}")
        if case_summary.get("payload_count") != state.payloads:
            raise CaptureFormatError(f"capture summary payload_count does not match case {case.id}")
        if case_summary.get("final_covered_checkpoint") != state.final_covered_checkpoint:
            raise CaptureFormatError(f"capture summary frontier does not match case {case.id}")
        if state.final_covered_checkpoint is None or state.final_covered_checkpoint < window_end:
            raise CaptureFormatError(f"case {case.id} did not cover window_end {window_end}")

    return ParsedCapture(header, summary, cases, parsed_frames, structural_reasons)


def run_snow_query(config: SnowflakeConfig, sql: str) -> list[dict]:
    process = subprocess.run(
        [
            "snow",
            "sql",
            "-c",
            config.connection,
            "--warehouse",
            config.warehouse,
            "-q",
            sql,
            "--format",
            "json",
        ],
        capture_output=True,
        text=True,
        timeout=900,
    )
    if process.returncode != 0:
        detail = process.stderr.strip() or process.stdout.strip() or f"exit {process.returncode}"
        raise SnowflakeError(f"snow sql failed: {detail}")
    try:
        rows = json.loads(process.stdout) if process.stdout.strip() else []
    except json.JSONDecodeError as error:
        raise SnowflakeError(f"snow sql returned invalid JSON: {error}") from error
    if not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows):
        raise SnowflakeError("snow sql JSON must be a list of objects")
    return rows


def _normalized_row(row: dict) -> dict:
    return {str(key).lower(): value for key, value in row.items()}


def _frontier(config: SnowflakeConfig, relation: str) -> int | None:
    rows = run_snow_query(
        config,
        f"SELECT MAX(CHECKPOINT) AS MAX_CHECKPOINT FROM {config.schema}.{relation}",
    )
    if not rows:
        return None
    value = _normalized_row(rows[0]).get("max_checkpoint")
    return int(value) if value is not None else None


def wait_for_warehouse(config: SnowflakeConfig, window_end: int) -> tuple[dict[str, int | None], list[str]]:
    frontiers = {"TRANSACTION": None, "EVENT": None}
    errors = []
    deadline = time.monotonic() + config.warehouse_wait_seconds
    while True:
        for relation in frontiers:
            if frontiers[relation] is not None and frontiers[relation] >= window_end:
                continue
            try:
                frontiers[relation] = _frontier(config, relation)
            except (SnowflakeError, subprocess.TimeoutExpired, OSError) as error:
                errors.append(str(error))
                return frontiers, errors
        if all(frontier is not None and frontier >= window_end for frontier in frontiers.values()):
            return frontiers, errors
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return frontiers, errors
        time.sleep(min(60, remaining))


def expected_sql(case: SubscriptionCase, schema: str, window_start: int, window_end: int) -> str:
    if case.rpc == "SubscribeCheckpoints":
        filter_sql = compile_filter_sql(case, "t")
        return (
            f"SELECT DISTINCT t.CHECKPOINT FROM {schema}.TRANSACTION t "
            f"WHERE t.CHECKPOINT >= {window_start} AND t.CHECKPOINT <= {window_end} "
            f"AND ({filter_sql})"
        )
    if case.rpc == "SubscribeTransactions":
        filter_sql = compile_filter_sql(case, "t")
        return (
            f"SELECT t.TRANSACTION_DIGEST, t.CHECKPOINT FROM {schema}.TRANSACTION t "
            f"WHERE t.CHECKPOINT >= {window_start} AND t.CHECKPOINT <= {window_end} "
            f"AND ({filter_sql})"
        )
    filter_sql = compile_filter_sql(case, "e")
    return (
        f"SELECT e.TRANSACTION_DIGEST, e.EVENT_INDEX, e.CHECKPOINT FROM {schema}.EVENT e "
        f"WHERE e.CHECKPOINT >= {window_start} AND e.CHECKPOINT <= {window_end} "
        f"AND ({filter_sql})"
    )


def expected_identities(case: SubscriptionCase, rows: list[dict]) -> set:
    identities = set()
    for row in rows:
        normalized = _normalized_row(row)
        try:
            if case.rpc == "SubscribeCheckpoints":
                identity = int(normalized["checkpoint"])
            elif case.rpc == "SubscribeTransactions":
                identity = (str(normalized["transaction_digest"]), int(normalized["checkpoint"]))
            else:
                identity = (
                    str(normalized["transaction_digest"]),
                    int(normalized["event_index"]),
                    int(normalized["checkpoint"]),
                )
        except (KeyError, TypeError, ValueError) as error:
            raise SnowflakeError(f"unexpected Snowflake row for {case.id}: {row!r}") from error
        identities.add(identity)
    return identities


def observed_identities(parsed: ParsedCapture) -> dict[str, set]:
    window_start = parsed.summary["window_start"]
    window_end = parsed.summary["window_end"]
    observed = {case.id: set() for case in parsed.cases}
    for case, response in parsed.frames:
        payload = payload_for(case, response)
        if payload is None:
            continue
        if case.rpc == "SubscribeCheckpoints":
            checkpoint = payload.sequence_number
        else:
            checkpoint = payload.checkpoint
        if window_start <= checkpoint <= window_end:
            observed[case.id].add(payload_identity(case, payload))
    return observed


def _identity_sample(identities: set, limit: int = 20) -> list:
    return [list(identity) if isinstance(identity, tuple) else identity for identity in sorted(identities)[:limit]]


def _write_result(result_path: Path, result: dict) -> None:
    result_path.parent.mkdir(parents=True, exist_ok=True)
    result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"results={result_path}", flush=True)


def verify_capture(capture_path: Path, result_path: Path, snowflake: SnowflakeConfig) -> int:
    cases = load_cases(snowflake.cases_path)
    parsed = parse_capture(capture_path, cases)
    capture_path = capture_path.expanduser().resolve()
    result_path = result_path.expanduser().resolve()
    observed = observed_identities(parsed)
    window_start = parsed.summary["window_start"]
    window_end = parsed.summary["window_end"]
    base_result = {
        "schema_version": SCHEMA_VERSION,
        "run_id": parsed.header["run_id"],
        "capture_path": str(capture_path),
        "window_start": window_start,
        "window_end": window_end,
        "warehouse_frontiers": {"TRANSACTION": None, "EVENT": None},
        "cases": [],
    }

    if not SCHEMA_RE.fullmatch(snowflake.schema):
        reason = f"invalid Snowflake schema {snowflake.schema!r}"
        for case in cases:
            base_result["cases"].append(
                {
                    "id": case.id,
                    "rpc": case.rpc,
                    "status": "INCONCLUSIVE",
                    "structural_reasons": [reason],
                    "observed_count": len(observed[case.id]),
                    "expected_count": None,
                    "missing": [],
                    "unexpected": [],
                }
            )
        _write_result(result_path, base_result)
        return 2

    frontiers, warehouse_errors = wait_for_warehouse(snowflake, window_end)
    base_result["warehouse_frontiers"] = frontiers
    relation_ready = {
        "TRANSACTION": frontiers["TRANSACTION"] is not None and frontiers["TRANSACTION"] >= window_end,
        "EVENT": frontiers["EVENT"] is not None and frontiers["EVENT"] >= window_end,
    }

    expected_by_case = {}
    query_error = warehouse_errors[0] if warehouse_errors else None
    if query_error is None:
        for case in cases:
            relation = "EVENT" if case.rpc == "SubscribeEvents" else "TRANSACTION"
            if not relation_ready[relation]:
                continue
            try:
                rows = run_snow_query(
                    snowflake,
                    expected_sql(case, snowflake.schema, window_start, window_end),
                )
                expected_by_case[case.id] = expected_identities(case, rows)
            except (SnowflakeError, subprocess.TimeoutExpired, OSError) as error:
                query_error = str(error)
                break

    statuses = {}
    for case in cases:
        relation = "EVENT" if case.rpc == "SubscribeEvents" else "TRANSACTION"
        reasons = list(parsed.structural_reasons[case.id])
        expected = expected_by_case.get(case.id)
        if query_error is not None:
            status = "INCONCLUSIVE"
            reasons.append(query_error)
            missing = set()
            unexpected = set()
        elif not relation_ready[relation]:
            status = "INCONCLUSIVE"
            frontier = frontiers[relation]
            reasons.append(
                f"{relation} frontier {frontier!r} did not reach window_end {window_end} "
                f"within {snowflake.warehouse_wait_seconds} seconds"
            )
            missing = set()
            unexpected = set()
        else:
            missing = expected - observed[case.id]
            unexpected = observed[case.id] - expected
            status = "FAIL" if reasons or missing or unexpected else "PASS"
        statuses[case.id] = status
        base_result["cases"].append(
            {
                "id": case.id,
                "rpc": case.rpc,
                "status": status,
                "structural_reasons": reasons,
                "observed_count": len(observed[case.id]),
                "expected_count": len(expected) if expected is not None else None,
                "missing": _identity_sample(missing),
                "unexpected": _identity_sample(unexpected),
            }
        )

    result_by_id = {record["id"]: record for record in base_result["cases"]}
    for tautology_id, unfiltered_id in (
        ("tx.sender.tautology", "tx.unfiltered"),
        ("ev.event_type.tautology", "ev.unfiltered"),
    ):
        if tautology_id not in observed or unfiltered_id not in observed:
            continue
        if observed[tautology_id] != observed[unfiltered_id]:
            record = result_by_id[tautology_id]
            record["structural_reasons"].append(
                f"observed set differs from metamorphic twin {unfiltered_id}"
            )
            if record["status"] != "INCONCLUSIVE":
                record["status"] = "FAIL"
                statuses[tautology_id] = "FAIL"

    _write_result(result_path, base_result)
    if any(record["status"] == "INCONCLUSIVE" for record in base_result["cases"]):
        return 2
    return 1 if any(record["status"] == "FAIL" for record in base_result["cases"]) else 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Record stable-v2 subscriptions and verify captured identities"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    record = subparsers.add_parser("record", help="record a bounded live subscription cohort")
    record.add_argument("target", help="SubscriptionService host:port")
    record.add_argument("--cases", type=Path, default=DEFAULT_CASES_PATH)
    record.add_argument("-o", "--out", type=Path)
    record.add_argument("--tls", action="store_true")
    record.add_argument("--ca", type=Path)
    record.add_argument("--server-name")
    record.add_argument("--rpc-deadline-seconds", type=int, default=900)
    record.add_argument("--observation-checkpoints", type=int, default=100)
    record.add_argument("--start-timeout-seconds", type=int, default=120)
    record.add_argument("--capture-timeout-seconds", type=int, default=600)

    verify = subparsers.add_parser("verify", help="verify a saved cohort against Snowflake")
    verify.add_argument("capture", type=Path)
    verify.add_argument("--cases", type=Path, default=DEFAULT_CASES_PATH)
    verify.add_argument("-o", "--out", type=Path)
    verify.add_argument("--snow-connection", default="nick")
    verify.add_argument("--snow-warehouse", default="ANALYTICS_WH")
    verify.add_argument("--snow-schema", default="CHAINDATA_TESTNET")
    verify.add_argument("--warehouse-wait-seconds", type=int, default=1800)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "record":
            cases = load_cases(args.cases)
            output_path = args.out or default_capture_path()
            return record_capture(
                cases,
                args.target,
                output_path,
                tls=args.tls,
                ca_path=args.ca,
                server_name=args.server_name,
                rpc_deadline_seconds=args.rpc_deadline_seconds,
                observation_checkpoints=args.observation_checkpoints,
                start_timeout_seconds=args.start_timeout_seconds,
                capture_timeout_seconds=args.capture_timeout_seconds,
            )

        capture_path = args.capture.expanduser().resolve()
        result_path = args.out or capture_path.with_name(f"{capture_path.stem}.results.json")
        snowflake = SnowflakeConfig(
            connection=args.snow_connection,
            warehouse=args.snow_warehouse,
            schema=args.snow_schema,
            warehouse_wait_seconds=args.warehouse_wait_seconds,
            cases_path=args.cases,
        )
        return verify_capture(capture_path, result_path, snowflake)
    except (FixtureError, CaptureFormatError, SnowflakeError, OSError, subprocess.TimeoutExpired) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
