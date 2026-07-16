# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import pytest

import leapfrog_benchmark as benchmark


class FakeStub:
    def __init__(self, pages):
        self.pages = pages
        self.requests = []

    def ListTransactions(self, request, timeout):
        copied_request = type(request)()
        copied_request.CopyFrom(request)
        self.requests.append(copied_request)
        return iter(self.pages[len(self.requests) - 1])


def terminal_response(cursor, reason, digest=None):
    response = benchmark.ledger.ListTransactionsResponse()
    response.watermark.cursor = cursor
    response.end.reason = reason
    if digest is not None:
        response.transaction.digest = digest
    return response


def test_exact_archival_request_shape():
    request = benchmark.build_request()

    assert request.start_checkpoint == 293_472_171
    assert request.end_checkpoint == 298_403_339
    assert list(request.read_mask.paths) == ["digest"]
    assert request.options.limit == 1
    assert request.options.ordering == benchmark.query_options.ORDERING_ASCENDING
    assert len(request.filter.terms) == 1
    assert len(request.filter.terms[0].literals) == 2
    assert request.filter.terms[0].literals[0].sender.address == benchmark.SENDER
    assert request.filter.terms[0].literals[1].move_call.function == benchmark.MOVE_CALL


def test_scan_limit_resumes_on_same_stub_and_counts_continuation():
    stub = FakeStub(
        [
            [
                terminal_response(
                    b"scan-frontier",
                    benchmark.query_options.QUERY_END_REASON_SCAN_LIMIT,
                )
            ],
            [
                terminal_response(
                    b"target",
                    benchmark.query_options.QUERY_END_REASON_ITEM_LIMIT,
                    benchmark.EXPECTED_DIGEST,
                )
            ],
        ]
    )

    result = benchmark.run_query(
        stub,
        benchmark.build_request(),
        timeout=1,
        max_continuations=10,
    )

    assert result.continuations == 1
    assert result.scan_limit_pages == 1
    assert len(result.page_latencies_ms) == 2
    assert not stub.requests[0].options.HasField("after")
    assert stub.requests[1].options.after == b"scan-frontier"


def test_match_on_first_page_needs_no_continuation():
    stub = FakeStub(
        [
            [
                terminal_response(
                    b"target",
                    benchmark.query_options.QUERY_END_REASON_ITEM_LIMIT,
                    benchmark.EXPECTED_DIGEST,
                )
            ]
        ]
    )

    result = benchmark.run_query(
        stub,
        benchmark.build_request(),
        timeout=1,
        max_continuations=10,
    )

    assert result.continuations == 0
    assert result.scan_limit_pages == 0
    assert len(stub.requests) == 1


def test_resumable_page_must_advance_cursor():
    stub = FakeStub(
        [
            [
                terminal_response(
                    b"scan-frontier",
                    benchmark.query_options.QUERY_END_REASON_SCAN_LIMIT,
                )
            ],
            [
                terminal_response(
                    b"scan-frontier",
                    benchmark.query_options.QUERY_END_REASON_SCAN_LIMIT,
                )
            ],
        ]
    )

    with pytest.raises(
        benchmark.ProtocolError,
        match="resumable QueryEnd did not advance watermark cursor",
    ):
        benchmark.run_query(
            stub,
            benchmark.build_request(),
            timeout=1,
            max_continuations=10,
        )
