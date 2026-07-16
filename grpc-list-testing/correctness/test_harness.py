# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for harness.py drain + oracle logic, using real proto messages
and a fake response stream (no network)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))

import harness as H
from sui.rpc.v2 import checkpoint_pb2, event_pb2, executed_transaction_pb2
from sui.rpc.v2 import ledger_service_pb2 as ls
from sui.rpc.v2 import query_options_pb2 as qo


# --- response builders --------------------------------------------------------

def tx_item(digest, cursor, checkpoint=None, end=None):
    r = ls.ListTransactionsResponse()
    r.transaction.digest = digest
    r.watermark.cursor = cursor
    if checkpoint is not None:
        r.watermark.checkpoint = checkpoint
    if end is not None:
        r.end.reason = end
    return r


def tx_wm(cursor, checkpoint=None):
    r = ls.ListTransactionsResponse()
    r.watermark.cursor = cursor
    if checkpoint is not None:
        r.watermark.checkpoint = checkpoint
    return r


def tx_end(reason, cursor, checkpoint=None):
    r = tx_wm(cursor, checkpoint)
    r.end.reason = reason
    return r


class FakeSend:
    """Maps the resume cursor (after for asc, before for desc) -> page responses."""
    def __init__(self, pages_by_key):
        self.pages = pages_by_key
        self.calls = []

    def __call__(self, req):
        if req.options.ordering == qo.ORDERING_DESCENDING:
            key = bytes(req.options.before)
        else:
            key = bytes(req.options.after)
        self.calls.append(key)
        return iter(self.pages[key])


def asc_req():
    return ls.ListTransactionsRequest()  # ordering defaults to ASCENDING (0)


def desc_req():
    r = ls.ListTransactionsRequest()
    r.options.ordering = qo.ORDERING_DESCENDING
    return r


def test_direct_payload_identities():
    transaction = executed_transaction_pb2.ExecutedTransaction(digest="tx")
    checkpoint = checkpoint_pb2.Checkpoint(sequence_number=7)
    event = event_pb2.Event(transaction_digest="tx", event_index=3)
    assert H.identity("ListTransactions", transaction) == ("tx", "tx")
    assert H.identity("ListCheckpoints", checkpoint) == ("cp", 7)
    assert H.identity("ListEvents", event) == ("ev", "tx", 3)


def test_event_identity_mask_replaces_request_mask():
    rec = {"rpc": "ListEvents", "request": {"read_mask": "eventType"}}
    request = H.base_request(rec, identity_mask=True)
    assert tuple(request.read_mask.paths) == ("transaction_digest", "event_index")


IL = qo.QUERY_END_REASON_ITEM_LIMIT
CB = qo.QUERY_END_REASON_CHECKPOINT_BOUND
SL = qo.QUERY_END_REASON_SCAN_LIMIT
LT = qo.QUERY_END_REASON_LEDGER_TIP


# --- drain --------------------------------------------------------------------

def test_drain_counts_item_limit_payload_before_resume():
    send = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=10),
            tx_item("B", b"c2", checkpoint=11),
            tx_item("C", b"c3", checkpoint=12, end=IL),
        ],
        b"c3": [
            tx_item("D", b"c4", checkpoint=13),
            tx_item("E", b"c5", checkpoint=14, end=CB),
        ],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C"), ("tx", "D"), ("tx", "E")]
    assert [page.count for page in r.pages] == [3, 2]
    assert r.terminal_reason == CB
    assert r.tiling_ok and r.watermark_ok
    assert send.calls == [b"", b"c3"]


def test_drain_detects_tiling_violation():
    send = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=1),
            tx_item("B", b"c2", checkpoint=2, end=IL),
        ],
        b"c2": [
            tx_item("B", b"c3", checkpoint=3),
            tx_item("C", b"c4", checkpoint=4, end=CB),
        ],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.tiling_ok is False
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C")]

def test_drain_detects_ascending_watermark_regression():
    send = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=10),
            tx_item("B", b"c2", checkpoint=5, end=CB),
        ],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.watermark_ok is False


def test_drain_detects_descending_watermark_regression():
    good = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=10),
            tx_item("B", b"c2", checkpoint=5, end=CB),
        ],
    })
    assert H.drain(good, "ListTransactions", desc_req()).watermark_ok is True

    bad = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=10),
            tx_item("B", b"c2", checkpoint=15, end=CB),
        ],
    })
    assert H.drain(bad, "ListTransactions", desc_req()).watermark_ok is False


def test_terminal_watermark_advances_resume_cursor():
    send = FakeSend({
        b"": [tx_end(SL, b"w1", checkpoint=100)],
        b"w1": [tx_item("Z", b"w2", checkpoint=200, end=CB)],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.ids == [("tx", "Z")]
    assert send.calls == [b"", b"w1"]


def test_drain_rejects_missing_watermark_cursor():
    response = ls.ListTransactionsResponse()
    response.transaction.digest = "A"
    response.watermark.SetInParent()
    response.end.reason = SL
    r = H.drain(FakeSend({b"": [response]}), "ListTransactions", asc_req())
    assert r.error == "watermark missing required cursor"
    assert r.watermark_ok is False
    assert r.ids == [("tx", "A")]


def test_drain_rejects_resumable_page_without_cursor_progress():
    send = FakeSend({
        b"": [tx_end(SL, b"c1", checkpoint=1)],
        b"c1": [tx_end(SL, b"c1", checkpoint=1)],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error == "resumable QueryEnd did not advance watermark cursor"
    assert send.calls == [b"", b"c1"]


def test_drain_requires_watermark_on_every_frame():
    missing = ls.ListTransactionsResponse()
    missing.transaction.digest = "A"
    send = FakeSend({b"": [missing, tx_end(CB, b"c1", checkpoint=1)]})
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.ids == [("tx", "A")]
    assert r.watermark_ok is False
    assert r.error == "response frame missing required watermark"


def test_drain_caps_at_max_drain():
    send = FakeSend({
        b"": [
            tx_item("A", b"c1", checkpoint=1),
            tx_item("B", b"c2", checkpoint=2, end=IL),
        ],
        b"c2": [tx_item("C", b"c3", checkpoint=3, end=CB)],
    })
    r = H.drain(send, "ListTransactions", asc_req(), max_drain=2)
    assert r.capped is True
    assert len(r.ids) == 2


def test_drain_single_page_mode():
    send = FakeSend({b"": [tx_end(SL, b"c1", checkpoint=5)]})
    r = H.drain(send, "ListTransactions", asc_req(), full=False)
    assert r.ids == []
    assert len(r.pages) == 1
    assert r.terminal_reason == SL


def test_drain_propagates_fatal_error():
    class Boom:
        def __call__(self, req):
            raise RuntimeError("INVALID_ARGUMENT: bad mask")  # non-retryable
    r = H.drain(Boom(), "ListTransactions", asc_req(), max_retries=0)
    assert r.error and "INVALID_ARGUMENT" in r.error


def test_drain_retries_transient_then_succeeds(monkeypatch):
    monkeypatch.setattr(H.time, "sleep", lambda *_: None)  # no real backoff in tests
    calls = {"n": 0}
    good = [
        tx_item("A", b"c1", checkpoint=1),
        tx_item("B", b"c2", checkpoint=2, end=CB),
    ]

    def flaky(req):
        calls["n"] += 1
        if calls["n"] == 1:
            raise RuntimeError("UNAVAILABLE: Connection refused")  # transient, retried
        return iter(good)
    r = H.drain(flaky, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A"), ("tx", "B")]
    assert calls["n"] == 2  # failed once, retried once


def test_drain_resumes_on_truncation(monkeypatch):
    # stream ends WITHOUT a QueryEnd (dropped conn) -> must resume, not accept as terminal
    monkeypatch.setattr(H.time, "sleep", lambda *_: None)
    calls = {"n": 0}

    def send(req):
        calls["n"] += 1
        if calls["n"] == 1:
            return iter([tx_item("A", b"c1", checkpoint=1), tx_item("B", b"c2", checkpoint=2)])
        return iter([tx_item("C", b"c3", checkpoint=3, end=CB)])
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C")]
    assert calls["n"] == 2  # resumed after the truncated first stream


def test_drain_truncation_without_progress_errors(monkeypatch):
    monkeypatch.setattr(H.time, "sleep", lambda *_: None)
    calls = {"n": 0}

    def send(req):
        calls["n"] += 1
        if calls["n"] == 1:
            return iter([tx_item("A", b"c1", checkpoint=1)])
        return iter([tx_item("B", b"c1", checkpoint=1)])

    r = H.drain(send, "ListTransactions", asc_req(), max_retries=2)
    assert r.error and "truncated" in r.error
    assert calls["n"] == 2


# --- oracle checks ------------------------------------------------------------

def _dr(ids, pages=None, **kw):
    r = H.DrainResult(ids=[("tx", x) for x in ids],
                      pages=pages or [H.PageMeta(len(ids), CB)])
    for k, v in kw.items():
        setattr(r, k, v)
    return r


def test_exact_count_pass_and_fail():
    rec = {"id": "c", "rpc": "ListTransactions",
           "oracle": {"kind": "exact_count", "expected_count": 3}}
    assert H.check_oracle(rec, {"c": _dr(["A", "B", "C"])}, 1000)[0] == "PASS"
    assert H.check_oracle(rec, {"c": _dr(["A", "B"])}, 1000)[0] == "FAIL"


def test_exact_count_first_page_reason():
    rec = {"id": "c", "rpc": "ListTransactions",
           "oracle": {"kind": "exact_count", "expected_count": 2,
                      "expected_end_reason": "QUERY_END_REASON_ITEM_LIMIT"}}
    good = _dr(["A", "B"], pages=[H.PageMeta(2, IL), H.PageMeta(0, CB)])
    bad = _dr(["A", "B"], pages=[H.PageMeta(2, CB)])
    assert H.check_oracle(rec, {"c": good}, 1000)[0] == "PASS"
    assert H.check_oracle(rec, {"c": bad}, 1000)[0] == "FAIL"


def test_exact_count_over_cap_skips():
    rec = {"id": "c", "rpc": "ListTransactions",
           "oracle": {"kind": "exact_count", "expected_count": 10_000_000}}
    dr = _dr(["A"], capped=True)
    status, _ = H.check_oracle(rec, {"c": dr}, 1000)
    assert status == "SKIP"


def test_limit_probe_first_page_only():
    rec = {"id": "c", "rpc": "ListTransactions",
           "request": {"options": {"limit": 5}},
           "oracle": {"kind": "exact_count", "expected_count": 47208,
                      "expected_end_reason": "QUERY_END_REASON_ITEM_LIMIT"}}
    good = _dr(["A", "B", "C", "D", "E"], pages=[H.PageMeta(5, IL)])
    assert H.check_oracle(rec, {"c": good}, 1000)[0] == "PASS"
    short = _dr(["A", "B"], pages=[H.PageMeta(2, IL)])
    assert H.check_oracle(rec, {"c": short}, 1000)[0] == "FAIL"
    wrong = _dr(["A", "B", "C", "D", "E"], pages=[H.PageMeta(5, CB)])
    assert H.check_oracle(rec, {"c": wrong}, 1000)[0] == "FAIL"


def test_degenerate():
    rec = {"id": "c", "rpc": "ListTransactions",
           "oracle": {"kind": "degenerate", "expected_count": 0,
                      "expected_end_reason": "QUERY_END_REASON_SCAN_LIMIT"}}
    good = _dr([], pages=[H.PageMeta(0, SL)])
    assert H.check_oracle(rec, {"c": good}, 1000)[0] == "PASS"
    nonempty = _dr(["A"], pages=[H.PageMeta(1, SL)])
    assert H.check_oracle(rec, {"c": nonempty}, 1000)[0] == "FAIL"


def test_decomposition_union():
    rec = {"id": "u", "rpc": "ListTransactions",
           "oracle": {"kind": "decomposition", "relation": "union",
                      "components": ["a", "b"]}}
    drains = {"u": _dr(["A", "B", "C"]), "a": _dr(["A", "B"]), "b": _dr(["B", "C"])}
    assert H.check_oracle(rec, drains, 1000)[0] == "PASS"
    drains["u"] = _dr(["A", "B"])  # missing C
    assert H.check_oracle(rec, drains, 1000)[0] == "FAIL"


def test_decomposition_difference():
    rec = {"id": "d", "rpc": "ListTransactions",
           "oracle": {"kind": "decomposition", "relation": "difference",
                      "components": ["all", "b"]}}
    drains = {"d": _dr(["A"]), "all": _dr(["A", "B"]), "b": _dr(["B"])}
    assert H.check_oracle(rec, drains, 1000)[0] == "PASS"


def test_decomposition_skips_on_capped_component():
    rec = {"id": "u", "rpc": "ListTransactions",
           "oracle": {"kind": "decomposition", "relation": "union",
                      "components": ["a", "b"]}}
    drains = {"u": _dr(["A"]), "a": _dr(["A"]), "b": _dr(["B"], capped=True)}
    assert H.check_oracle(rec, drains, 1000)[0] == "SKIP"


# --- pairwise invariants ------------------------------------------------------

def test_asc_desc_order_invariant():
    recs = [{"id": "x.asc.1", "oracle": {}}, {"id": "x.desc.1", "oracle": {}}]
    drains = {"x.asc.1": _dr(["A", "B", "C"]), "x.desc.1": _dr(["C", "B", "A"])}
    res = H.pair_invariants(recs, drains)
    assert len(res) == 1 and res[0].status == "PASS"
    drains["x.desc.1"] = _dr(["C", "A", "B"])  # same set, wrong order
    res = H.pair_invariants(recs, drains)
    assert res[0].status == "FAIL" and "ordering" in res[0].reasons[0]


def test_readmask_agreement_invariant():
    recs = [{"id": "x.cheap.1", "oracle": {}}, {"id": "x.expensive.1", "oracle": {}}]
    drains = {"x.cheap.1": _dr(["A", "B"]), "x.expensive.1": _dr(["A", "B"])}
    res = H.pair_invariants(recs, drains)
    assert any(r.cid.startswith("readmask") and r.status == "PASS" for r in res)
    drains["x.expensive.1"] = _dr(["A"])  # set differs by mask -> bug
    res = H.pair_invariants(recs, drains)
    assert any(r.cid.startswith("readmask") and r.status == "FAIL" for r in res)
