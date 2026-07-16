# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""No-network tests for the subscription correctness workflow."""

import json
import queue
import threading
import time
from pathlib import Path

import pytest
from google.protobuf import json_format

import subscription_harness as H
from sui.rpc.v2 import subscription_service_pb2 as ss


SYSTEM_ADDRESS = "0x" + "0" * 64
SYSTEM_EVENT_TYPE = "0x" + "0" * 63 + "3::validator::StakingRequestEvent"
EXPECTED_CASE_IDS = [
    "cp.unfiltered",
    "cp.sender.system",
    "tx.unfiltered",
    "tx.sender.system",
    "tx.sender.not_system",
    "tx.sender.tautology",
    "ev.unfiltered",
    "ev.sender.system",
    "ev.sender.not_system",
    "ev.event_type.tautology",
    "ev.emit_module.not_sui_system",
]


def case_from_dict(case_id, rpc, request):
    message = json_format.ParseDict(request, H.REQUEST_TYPES[rpc]())
    return H.SubscriptionCase(case_id, rpc, message)


def checkpoint_response(cursor=None, sequence=None, digest="checkpoint"):
    response = ss.SubscribeCheckpointsResponse()
    if cursor is not None:
        response.cursor = cursor
    if sequence is not None:
        response.checkpoint.sequence_number = sequence
        response.checkpoint.digest = digest
    return response


def transaction_response(cursor=b"cursor", covered=None, digest=None, checkpoint=0, index=0):
    response = ss.SubscribeTransactionsResponse()
    response.watermark.cursor = cursor
    if covered is not None:
        response.watermark.checkpoint = covered
    if digest is not None:
        response.transaction.digest = digest
        response.transaction.checkpoint = checkpoint
        response.transaction.transaction_index = index
    return response


def event_response(cursor=b"cursor", covered=None, digest=None, checkpoint=0,
                   transaction_index=0, event_index=0):
    response = ss.SubscribeEventsResponse()
    response.watermark.cursor = cursor
    if covered is not None:
        response.watermark.checkpoint = covered
    if digest is not None:
        response.event.transaction_digest = digest
        response.event.checkpoint = checkpoint
        response.event.transaction_index = transaction_index
        response.event.event_index = event_index
    return response


def write_records(path, records):
    path.write_text("".join(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n"
                            for record in records))


def fixture_records():
    return [json.loads(line) for line in H.DEFAULT_CASES_PATH.read_text().splitlines() if line]


def build_capture(path):
    cases = H.load_cases(H.DEFAULT_CASES_PATH)
    run_id = "test-run"
    records = [{
        "type": "header",
        "schema_version": H.SCHEMA_VERSION,
        "run_id": run_id,
        "started_at": "2026-01-01T00:00:00.000Z",
        "target": "fake:9000",
        "cases": [case.as_dict() for case in cases],
    }]
    sequence = 0
    summaries = []
    for case_index, case in enumerate(cases, 1):
        responses = []
        if case.filtered:
            if case.rpc == "SubscribeCheckpoints":
                responses.append(checkpoint_response(cursor=9))
            elif case.rpc == "SubscribeTransactions":
                responses.append(transaction_response(cursor=bytes([case_index, 1])))
            else:
                responses.append(event_response(cursor=bytes([case_index, 1])))
        if case.rpc == "SubscribeCheckpoints":
            responses.append(checkpoint_response(cursor=10, sequence=10))
        elif case.rpc == "SubscribeTransactions":
            responses.append(transaction_response(
                cursor=bytes([case_index, 2]), covered=10, digest="tx-shared", checkpoint=10, index=0
            ))
        else:
            responses.append(event_response(
                cursor=bytes([case_index, 2]), covered=10, digest="event-tx-shared",
                checkpoint=10, transaction_index=0, event_index=0,
            ))
        for response in responses:
            sequence += 1
            records.append({
                "type": "frame",
                "case_id": case.id,
                "rpc": case.rpc,
                "receive_sequence": sequence,
                "received_at": "2026-01-01T00:00:01.000Z",
                "response": H.message_to_dict(response),
            })
        summaries.append({
            "case_id": case.id,
            "rpc": case.rpc,
            "frame_count": len(responses),
            "payload_count": 1,
            "final_covered_checkpoint": 10,
        })
    records.append({
        "type": "summary",
        "schema_version": H.SCHEMA_VERSION,
        "run_id": run_id,
        "finished_at": "2026-01-01T00:00:02.000Z",
        "window_start": 10,
        "window_end": 10,
        "deliberate_cancellation": True,
        "capture_errors": [],
        "case_summaries": summaries,
    })
    write_records(path, records)
    return records


def identities_to_rows(case, identities, lower_case=False):
    rows = []
    for identity in sorted(identities):
        if case.rpc == "SubscribeCheckpoints":
            row = {"checkpoint" if lower_case else "CHECKPOINT": identity}
        elif case.rpc == "SubscribeTransactions":
            digest, checkpoint = identity
            row = {
                "transaction_digest" if lower_case else "TRANSACTION_DIGEST": digest,
                "checkpoint" if lower_case else "CHECKPOINT": checkpoint,
            }
        else:
            digest, event_index, checkpoint = identity
            row = {
                "transaction_digest" if lower_case else "TRANSACTION_DIGEST": digest,
                "event_index" if lower_case else "EVENT_INDEX": event_index,
                "checkpoint" if lower_case else "CHECKPOINT": checkpoint,
            }
        rows.append(row)
    return rows


def install_snow_mock(monkeypatch, capture_path, *, transaction_frontier=10, event_frontier=10,
                      expected_overrides=None):
    cases = H.load_cases(H.DEFAULT_CASES_PATH)
    parsed = H.parse_capture(capture_path, cases)
    observed = H.observed_identities(parsed)
    expected_overrides = expected_overrides or {}
    queries = {}
    for index, case in enumerate(cases):
        identities = expected_overrides.get(case.id, observed[case.id])
        sql = H.expected_sql(case, "CHAINDATA_TESTNET", 10, 10)
        queries[sql] = identities_to_rows(case, identities, lower_case=index % 2 == 1)

    def fake_run(config, sql):
        if "MAX(CHECKPOINT)" in sql and ".TRANSACTION" in sql:
            return [{"MAX_CHECKPOINT": transaction_frontier}]
        if "MAX(CHECKPOINT)" in sql and ".EVENT" in sql:
            return [{"max_checkpoint": event_frontier}]
        return queries[sql]

    monkeypatch.setattr(H, "run_snow_query", fake_run)


# --- fixture parsing ---------------------------------------------------------


def test_loads_all_generated_fixture_requests():
    cases = H.load_cases(H.DEFAULT_CASES_PATH)
    assert [case.id for case in cases] == EXPECTED_CASE_IDS
    assert all(type(case.request) is H.REQUEST_TYPES[case.rpc] for case in cases)


def test_rejects_duplicate_case_ids(tmp_path):
    record = fixture_records()[0]
    path = tmp_path / "duplicate.jsonl"
    write_records(path, [record, record])
    with pytest.raises(H.FixtureError, match="duplicate id"):
        H.load_cases(path)


@pytest.mark.parametrize(
    "record,error",
    [
        ({"id": "bad", "rpc": "SubscribeTransactions", "request": {}, "oracle": {}}, "keys must be exactly"),
        ({"id": "bad", "rpc": "SubscribeTransactions", "request": {"start_checkpoint": 1}}, "invalid SubscribeTransactions"),
        ({"id": "bad", "rpc": "SubscribeTransactions", "request": {"filter": {"terms": []}}}, "at least one term"),
        ({"id": "bad", "rpc": "SubscribeEvents", "request": {"filter": {"terms": [{"literals": []}]}}}, "at least one literal"),
        ({
            "id": "bad", "rpc": "SubscribeTransactions",
            "request": {"filter": {"terms": [{"literals": [{"move_call": {"function": "0x2::m::f"}}]}]}},
        }, "unsupported transaction predicate"),
        ({
            "id": "bad", "rpc": "SubscribeEvents",
            "request": {"filter": {"terms": [{"literals": [{"emit_module": {"module": "0x2"}}]}]}},
        }, "must contain package and module"),
        ({
            "id": "bad", "rpc": "SubscribeEvents",
            "request": {"filter": {"terms": [{"literals": [{"event_type": {"event_type": "0x2::m::E<"}}]}]}},
        }, "unbalanced generic brackets"),
    ],
)
def test_rejects_invalid_fixture_surface(tmp_path, record, error):
    path = tmp_path / "invalid.jsonl"
    write_records(path, [record])
    with pytest.raises(H.FixtureError, match=error):
        H.load_cases(path)


# --- structural frame validation --------------------------------------------


def test_checkpoint_requires_cursor_and_payload_cursor_equality():
    case = case_from_dict("cp", "SubscribeCheckpoints", {"read_mask": "sequenceNumber,digest"})
    missing = checkpoint_response(sequence=7)
    reasons = H.validate_frame(case, missing, H.FrameState())
    assert "checkpoint frame is missing cursor" in reasons

    mismatch = checkpoint_response(cursor=8, sequence=7)
    reasons = H.validate_frame(case, mismatch, H.FrameState())
    assert any("does not equal cursor" in reason for reason in reasons)


def test_filtered_checkpoint_starts_progress_only_and_unfiltered_is_consecutive():
    filtered = case_from_dict(
        "cp.filtered", "SubscribeCheckpoints",
        {"filter": {"terms": [{"literals": [{"sender": {"address": SYSTEM_ADDRESS}}]}]}},
    )
    state = H.FrameState()
    assert H.validate_frame(filtered, checkpoint_response(cursor=10), state) == []
    assert state.ready
    bad_first = H.validate_frame(filtered, checkpoint_response(cursor=10, sequence=10), H.FrameState())
    assert any("progress-only" in reason for reason in bad_first)

    unfiltered = case_from_dict("cp", "SubscribeCheckpoints", {"read_mask": "sequenceNumber,digest"})
    state = H.FrameState()
    assert H.validate_frame(unfiltered, checkpoint_response(cursor=10, sequence=10), state) == []
    reasons = H.validate_frame(unfiltered, checkpoint_response(cursor=12, sequence=12), state)
    assert any("did not follow" in reason for reason in reasons)


def test_transaction_watermark_cursor_and_payload_identity_fields_are_required():
    case = case_from_dict("tx", "SubscribeTransactions", {})
    no_watermark = ss.SubscribeTransactionsResponse()
    assert any("missing watermark" in reason for reason in H.validate_frame(case, no_watermark, H.FrameState()))

    no_cursor = ss.SubscribeTransactionsResponse()
    no_cursor.watermark.SetInParent()
    assert any("missing cursor" in reason for reason in H.validate_frame(case, no_cursor, H.FrameState()))

    missing_position = ss.SubscribeTransactionsResponse()
    missing_position.watermark.cursor = b"c"
    missing_position.transaction.digest = "tx"
    reasons = H.validate_frame(case, missing_position, H.FrameState())
    assert "transaction payload is missing checkpoint" in reasons
    assert "transaction payload is missing transaction_index" in reasons


def test_unfiltered_transaction_and_event_allow_payload_free_ticks():
    transaction_case = case_from_dict("tx", "SubscribeTransactions", {})
    transaction_state = H.FrameState()
    assert H.validate_frame(transaction_case, transaction_response(), transaction_state) == []
    assert transaction_state.ready and transaction_state.payloads == 0

    event_case = case_from_dict("ev", "SubscribeEvents", {})
    event_state = H.FrameState()
    assert H.validate_frame(event_case, event_response(), event_state) == []
    assert event_state.ready and event_state.payloads == 0


def test_filtered_transaction_first_frame_has_no_payload_or_covered_checkpoint():
    case = case_from_dict(
        "tx.filtered", "SubscribeTransactions",
        {"filter": {"terms": [{"literals": [{"sender": {"address": SYSTEM_ADDRESS}}]}]}},
    )
    good_state = H.FrameState()
    assert H.validate_frame(case, transaction_response(), good_state) == []
    assert good_state.ready

    reasons = H.validate_frame(
        case,
        transaction_response(covered=10, digest="tx", checkpoint=10, index=0),
        H.FrameState(),
    )
    assert any("progress-only" in reason for reason in reasons)
    assert any("unexpectedly covered" in reason for reason in reasons)


def test_event_identity_ordering_frontier_and_duplicate_checks():
    case = case_from_dict("ev", "SubscribeEvents", {})
    state = H.FrameState()
    first = event_response(covered=10, digest="tx", checkpoint=10, transaction_index=0, event_index=0)
    assert H.validate_frame(case, first, state) == []

    regression = event_response(
        cursor=b"two", covered=9, digest="tx2", checkpoint=9, transaction_index=0, event_index=0
    )
    reasons = H.validate_frame(case, regression, state)
    assert any("covered checkpoint regressed" in reason for reason in reasons)
    assert any("did not follow" in reason for reason in reasons)

    duplicate = event_response(
        cursor=b"three", covered=10, digest="tx", checkpoint=10, transaction_index=0, event_index=0
    )
    reasons = H.validate_frame(case, duplicate, state)
    assert any("duplicate payload identity" in reason for reason in reasons)


def test_mask_conditioned_event_fields_are_required():
    case = case_from_dict(
        "ev", "SubscribeEvents",
        {"read_mask": "transactionDigest,eventIndex,checkpoint,transactionIndex"},
    )
    response = ss.SubscribeEventsResponse()
    response.watermark.cursor = b"cursor"
    response.event.transaction_digest = "tx"
    reasons = H.validate_frame(case, response, H.FrameState())
    assert "event payload is missing event_index" in reasons
    assert "event payload is missing checkpoint" in reasons
    assert "event payload is missing transaction_index" in reasons


# --- record orchestration ----------------------------------------------------


_STOP = object()


class PushCall:
    def __init__(self):
        self.items = queue.Queue()
        self.cancelled = False

    def __iter__(self):
        return self

    def __next__(self):
        item = self.items.get(timeout=5)
        if item is _STOP:
            raise StopIteration
        if isinstance(item, BaseException):
            raise item
        return item

    def push(self, item):
        self.items.put(item)

    def cancel(self):
        self.cancelled = True
        self.items.put(_STOP)
        return True


class FakeBackend:
    def __init__(self, cases):
        self.calls = {case.id: PushCall() for case in cases}
        self.closed = False

    def open(self, case):
        return self.calls[case.id]

    def close(self):
        self.closed = True


class FakeRpcError(RuntimeError):
    class Code:
        name = "UNAVAILABLE"

    def code(self):
        return self.Code()


def backend_factory(backend):
    return lambda *args, **kwargs: backend


def wait_for_lines(path, count):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if path.exists() and len(path.read_text().splitlines()) >= count:
            return
        time.sleep(0.005)
    raise AssertionError(f"{path} did not reach {count} lines")


def record_cases():
    return [
        case_from_dict("cp.unfiltered", "SubscribeCheckpoints", {"read_mask": "sequenceNumber,digest"}),
        case_from_dict("tx.unfiltered", "SubscribeTransactions", {"read_mask": "digest,checkpoint,transactionIndex"}),
    ]


def test_record_fences_readiness_uses_extra_checkpoint_and_cancels(tmp_path):
    cases = record_cases()
    backend = FakeBackend(cases)
    output = tmp_path / "capture.jsonl"
    result = []

    thread = threading.Thread(target=lambda: result.append(H.record_capture(
        cases, "fake:9000", output, observation_checkpoints=2,
        start_timeout_seconds=2, capture_timeout_seconds=2,
        backend_factory=backend_factory(backend),
    )))
    thread.start()
    wait_for_lines(output, 1)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=10, sequence=10))
    backend.calls["tx.unfiltered"].push(transaction_response(cursor=b"ready"))
    wait_for_lines(output, 3)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=11, sequence=11))
    wait_for_lines(output, 4)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=12, sequence=12))
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=13, sequence=13))
    backend.calls["tx.unfiltered"].push(transaction_response(cursor=b"covered", covered=13))
    thread.join(timeout=5)

    assert result == [0]
    assert not thread.is_alive()
    records = [json.loads(line) for line in output.read_text().splitlines()]
    summary = records[-1]
    assert summary["window_start"] == 12
    assert summary["window_end"] == 13
    assert summary["capture_errors"] == []
    assert summary["deliberate_cancellation"] is True
    assert all(call.cancelled for call in backend.calls.values())
    assert backend.closed


def test_record_start_timeout(tmp_path):
    cases = [record_cases()[0]]
    backend = FakeBackend(cases)
    output = tmp_path / "start-timeout.jsonl"
    rc = H.record_capture(
        cases, "fake:9000", output, start_timeout_seconds=0.02,
        capture_timeout_seconds=1, backend_factory=backend_factory(backend),
    )
    assert rc == 1
    summary = json.loads(output.read_text().splitlines()[-1])
    assert summary["capture_errors"] == ["start timeout after 0.02 seconds"]


def test_record_capture_timeout_after_window_start(tmp_path):
    cases = [record_cases()[0]]
    backend = FakeBackend(cases)
    output = tmp_path / "capture-timeout.jsonl"
    result = []
    thread = threading.Thread(target=lambda: result.append(H.record_capture(
        cases, "fake:9000", output, observation_checkpoints=2,
        start_timeout_seconds=1, capture_timeout_seconds=0.08,
        backend_factory=backend_factory(backend),
    )))
    thread.start()
    wait_for_lines(output, 1)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=10, sequence=10))
    wait_for_lines(output, 2)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=11, sequence=11))
    wait_for_lines(output, 3)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=12, sequence=12))
    thread.join(timeout=5)
    assert result == [1]
    summary = json.loads(output.read_text().splitlines()[-1])
    assert summary["window_start"] == 12
    assert summary["capture_errors"] == ["capture timeout after 0.08 seconds"]


def test_record_rejects_premature_eof(tmp_path):
    cases = [record_cases()[0]]
    backend = FakeBackend(cases)
    backend.calls["cp.unfiltered"].push(checkpoint_response(cursor=10, sequence=10))
    backend.calls["cp.unfiltered"].push(_STOP)
    output = tmp_path / "eof.jsonl"
    assert H.record_capture(
        cases, "fake:9000", output, start_timeout_seconds=1, capture_timeout_seconds=1,
        backend_factory=backend_factory(backend),
    ) == 1
    summary = json.loads(output.read_text().splitlines()[-1])
    assert summary["capture_errors"] == ["cp.unfiltered: stream ended before common coverage"]


def test_record_rejects_precoverage_grpc_error(tmp_path):
    cases = [record_cases()[0]]
    backend = FakeBackend(cases)
    backend.calls["cp.unfiltered"].push(FakeRpcError("server lagged subscriber"))
    output = tmp_path / "grpc-error.jsonl"
    assert H.record_capture(
        cases, "fake:9000", output, start_timeout_seconds=1, capture_timeout_seconds=1,
        backend_factory=backend_factory(backend),
    ) == 1
    summary = json.loads(output.read_text().splitlines()[-1])
    assert "gRPC UNAVAILABLE" in summary["capture_errors"][0]


# --- capture logs ------------------------------------------------------------


def test_capture_round_trip(tmp_path):
    path = tmp_path / "capture.jsonl"
    build_capture(path)
    cases = H.load_cases(H.DEFAULT_CASES_PATH)
    parsed = H.parse_capture(path, cases)
    assert parsed.header["run_id"] == "test-run"
    assert len(parsed.frames) == 19
    assert all(not reasons for reasons in parsed.structural_reasons.values())
    assert H.observed_identities(parsed)["tx.sender.tautology"] == {("tx-shared", 10)}


@pytest.mark.parametrize("mutation", ["truncated", "noncontiguous", "unknown_case", "header_mismatch", "incomplete"])
def test_rejects_malformed_or_incomplete_capture(tmp_path, mutation):
    path = tmp_path / "capture.jsonl"
    records = build_capture(path)
    if mutation == "truncated":
        records.pop()
    elif mutation == "noncontiguous":
        records[1]["receive_sequence"] = 2
    elif mutation == "unknown_case":
        records[1]["case_id"] = "unknown"
    elif mutation == "header_mismatch":
        records[0]["cases"][0]["request"] = {}
    else:
        records[-1]["case_summaries"][0]["final_covered_checkpoint"] = 9
    write_records(path, records)
    with pytest.raises(H.CaptureFormatError):
        H.parse_capture(path, H.load_cases(H.DEFAULT_CASES_PATH))


def test_duplicate_and_payload_checkpoint_disagreement_are_structural_failures(tmp_path):
    path = tmp_path / "capture.jsonl"
    records = build_capture(path)
    summary = records.pop()
    duplicate_source = next(
        record for record in records
        if record.get("case_id") == "tx.sender.system" and "transaction" in record.get("response", {})
    )
    duplicate = json.loads(json.dumps(duplicate_source))
    duplicate["receive_sequence"] = max(record.get("receive_sequence", 0) for record in records) + 1
    records.append(duplicate)
    tx_summary = next(item for item in summary["case_summaries"] if item["case_id"] == "tx.sender.system")
    tx_summary["frame_count"] += 1
    tx_summary["payload_count"] += 1

    checkpoint_frame = next(record for record in records if record.get("case_id") == "cp.unfiltered")
    checkpoint_frame["response"]["cursor"] = "11"
    cp_summary = next(item for item in summary["case_summaries"] if item["case_id"] == "cp.unfiltered")
    cp_summary["final_covered_checkpoint"] = 11
    records.append(summary)
    write_records(path, records)

    parsed = H.parse_capture(path, H.load_cases(H.DEFAULT_CASES_PATH))
    assert any("duplicate payload identity" in reason
               for reason in parsed.structural_reasons["tx.sender.system"])
    assert any("does not equal cursor" in reason
               for reason in parsed.structural_reasons["cp.unfiltered"])


# --- filter SQL --------------------------------------------------------------


def test_sender_dnf_negation_and_null_handling_sql():
    cases = {case.id: case for case in H.load_cases(H.DEFAULT_CASES_PATH)}
    sender = f"t.SENDER = '{SYSTEM_ADDRESS}'"
    coalesced = f"COALESCE(({sender}), FALSE)"
    assert H.compile_filter_sql(cases["tx.unfiltered"], "t") == "TRUE"
    assert H.compile_filter_sql(cases["tx.sender.system"], "t") == f"(({coalesced}))"
    assert H.compile_filter_sql(cases["tx.sender.not_system"], "t") == f"((NOT ({coalesced})))"
    assert H.compile_filter_sql(cases["tx.sender.tautology"], "t") == (
        f"(({coalesced}) OR (NOT ({coalesced})))"
    )


def test_emit_module_and_event_type_specificity_sql():
    cases = {case.id: case for case in H.load_cases(H.DEFAULT_CASES_PATH)}
    module_predicate = (
        "e.PACKAGE = '0x0000000000000000000000000000000000000000000000000000000000000003' "
        "AND e.MODULE = 'sui_system'"
    )
    assert H.compile_filter_sql(cases["ev.emit_module.not_sui_system"], "e") == (
        f"((NOT (COALESCE(({module_predicate}), FALSE))))"
    )

    address_case = case_from_dict(
        "address", "SubscribeEvents",
        {"filter": {"terms": [{"literals": [{"event_type": {"event_type": "0x2"}}]}]}},
    )
    module_case = case_from_dict(
        "module", "SubscribeEvents",
        {"filter": {"terms": [{"literals": [{"event_type": {"event_type": "0x2::coin"}}]}]}},
    )
    name_case = case_from_dict(
        "name", "SubscribeEvents",
        {"filter": {"terms": [{"literals": [{"event_type": {"event_type": "0x2::coin::Created"}}]}]}},
    )
    generic_case = case_from_dict(
        "generic", "SubscribeEvents",
        {"filter": {"terms": [{"literals": [{
            "event_type": {"event_type": "0x2::coin::Created<0x2::sui::SUI>"}
        }]}]}},
    )
    assert "STARTSWITH(e.EVENT_TYPE, '0x2::')" in H.compile_filter_sql(address_case, "e")
    assert "STARTSWITH(e.EVENT_TYPE, '0x2::coin::')" in H.compile_filter_sql(module_case, "e")
    assert "e.EVENT_TYPE = '0x2::coin::Created' OR STARTSWITH" in H.compile_filter_sql(name_case, "e")
    generic_sql = H.compile_filter_sql(generic_case, "e")
    assert "e.EVENT_TYPE = '0x2::coin::Created<0x2::sui::SUI>'" in generic_sql
    assert "STARTSWITH" not in generic_sql


def test_filter_sql_rejects_quotes_before_querying():
    case = case_from_dict(
        "quote", "SubscribeEvents",
        {"filter": {"terms": [{"literals": [{"sender": {"address": "0x2' OR TRUE"}}]}]}},
    )
    with pytest.raises(H.FixtureError, match="must not contain a quote"):
        H.compile_filter_sql(case, "e")


def test_expected_sql_uses_inclusive_primary_relation_window():
    cases = {case.id: case for case in H.load_cases(H.DEFAULT_CASES_PATH)}
    assert H.expected_sql(cases["cp.unfiltered"], "S", 4, 9) == (
        "SELECT DISTINCT t.CHECKPOINT FROM S.TRANSACTION t "
        "WHERE t.CHECKPOINT >= 4 AND t.CHECKPOINT <= 9 AND (TRUE)"
    )
    assert H.expected_sql(cases["tx.unfiltered"], "S", 4, 9) == (
        "SELECT t.TRANSACTION_DIGEST, t.CHECKPOINT FROM S.TRANSACTION t "
        "WHERE t.CHECKPOINT >= 4 AND t.CHECKPOINT <= 9 AND (TRUE)"
    )
    assert H.expected_sql(cases["ev.unfiltered"], "S", 4, 9) == (
        "SELECT e.TRANSACTION_DIGEST, e.EVENT_INDEX, e.CHECKPOINT FROM S.EVENT e "
        "WHERE e.CHECKPOINT >= 4 AND e.CHECKPOINT <= 9 AND (TRUE)"
    )


# --- Snowflake verification -------------------------------------------------


def test_verify_exact_sets_with_mixed_case_snowflake_columns(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    result = tmp_path / "results.json"
    build_capture(capture)
    install_snow_mock(monkeypatch, capture)
    config = H.SnowflakeConfig(warehouse_wait_seconds=0)
    assert H.verify_capture(capture, result, config) == 0
    output = json.loads(result.read_text())
    assert {case["status"] for case in output["cases"]} == {"PASS"}
    assert output["warehouse_frontiers"] == {"TRANSACTION": 10, "EVENT": 10}


def test_verify_handles_independent_transaction_and_event_lag(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    result = tmp_path / "results.json"
    build_capture(capture)
    install_snow_mock(monkeypatch, capture, transaction_frontier=10, event_frontier=9)
    assert H.verify_capture(
        capture, result, H.SnowflakeConfig(warehouse_wait_seconds=0)
    ) == 2
    output = json.loads(result.read_text())
    statuses = {case["id"]: case["status"] for case in output["cases"]}
    assert all(statuses[case_id] == "PASS" for case_id in EXPECTED_CASE_IDS if not case_id.startswith("ev."))
    assert all(statuses[case_id] == "INCONCLUSIVE" for case_id in EXPECTED_CASE_IDS if case_id.startswith("ev."))


def test_verify_reports_bounded_missing_and_unexpected_sets(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    result = tmp_path / "results.json"
    build_capture(capture)
    expected = {("missing", 10)}
    install_snow_mock(monkeypatch, capture, expected_overrides={"tx.sender.system": expected})
    assert H.verify_capture(
        capture, result, H.SnowflakeConfig(warehouse_wait_seconds=0)
    ) == 1
    output = json.loads(result.read_text())
    case = next(case for case in output["cases"] if case["id"] == "tx.sender.system")
    assert case["status"] == "FAIL"
    assert case["missing"] == [["missing", 10]]
    assert case["unexpected"] == [["tx-shared", 10]]


def test_verify_reports_duplicate_structural_failure(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    records = build_capture(capture)
    summary = records.pop()
    source = next(
        record for record in records
        if record.get("case_id") == "tx.sender.system" and "transaction" in record.get("response", {})
    )
    duplicate = json.loads(json.dumps(source))
    duplicate["receive_sequence"] = max(record.get("receive_sequence", 0) for record in records) + 1
    records.append(duplicate)
    case_summary = next(item for item in summary["case_summaries"] if item["case_id"] == "tx.sender.system")
    case_summary["frame_count"] += 1
    case_summary["payload_count"] += 1
    records.append(summary)
    write_records(capture, records)
    result = tmp_path / "results.json"
    install_snow_mock(monkeypatch, capture)
    assert H.verify_capture(
        capture, result, H.SnowflakeConfig(warehouse_wait_seconds=0)
    ) == 1
    output = json.loads(result.read_text())
    case = next(case for case in output["cases"] if case["id"] == "tx.sender.system")
    assert case["status"] == "FAIL"
    assert any("duplicate payload identity" in reason for reason in case["structural_reasons"])


def test_verify_reports_tautology_mismatch_even_when_oracles_match(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    records = build_capture(capture)
    frame = next(
        record for record in records
        if record.get("case_id") == "tx.sender.tautology" and "transaction" in record.get("response", {})
    )
    frame["response"]["transaction"]["digest"] = "different"
    write_records(capture, records)
    result = tmp_path / "results.json"
    install_snow_mock(monkeypatch, capture)
    assert H.verify_capture(
        capture, result, H.SnowflakeConfig(warehouse_wait_seconds=0)
    ) == 1
    output = json.loads(result.read_text())
    case = next(case for case in output["cases"] if case["id"] == "tx.sender.tautology")
    assert case["status"] == "FAIL"
    assert any("metamorphic twin" in reason for reason in case["structural_reasons"])


def test_verify_snowflake_failure_is_inconclusive_and_writes_result(tmp_path, monkeypatch):
    capture = tmp_path / "capture.jsonl"
    result = tmp_path / "results.json"
    build_capture(capture)

    def fail_query(config, sql):
        raise H.SnowflakeError("authentication failed")

    monkeypatch.setattr(H, "run_snow_query", fail_query)
    assert H.verify_capture(
        capture, result, H.SnowflakeConfig(warehouse_wait_seconds=0)
    ) == 2
    output = json.loads(result.read_text())
    assert {case["status"] for case in output["cases"]} == {"INCONCLUSIVE"}
    assert all("authentication failed" in case["structural_reasons"] for case in output["cases"])


def test_main_returns_two_for_malformed_capture_without_query(tmp_path, monkeypatch):
    capture = tmp_path / "truncated.jsonl"
    capture.write_text('{"type":"header"}\n')
    monkeypatch.setattr(H, "run_snow_query", lambda config, sql: pytest.fail("Snowflake must not run"))
    assert H.main(["verify", str(capture), "--warehouse-wait-seconds", "0"]) == 2
