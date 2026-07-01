# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for harness.py drain + oracle logic, using real proto messages
and a fake response stream (no network)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))

import harness as H
from sui.rpc.v2alpha import ledger_service_pb2 as ls
from sui.rpc.v2alpha import query_options_pb2 as qo


# --- response builders --------------------------------------------------------

def tx_item(digest, cursor=None, hi=None, lo=None):
    r = ls.ListTransactionsResponse()
    r.item.transaction.digest = digest
    if cursor is not None:
        r.item.watermark.cursor = cursor
    if hi is not None:
        r.item.watermark.checkpoint_hi = hi
    if lo is not None:
        r.item.watermark.checkpoint_lo = lo
    return r


def tx_wm(cursor, hi=None, lo=None):
    r = ls.ListTransactionsResponse()
    r.watermark.cursor = cursor
    if hi is not None:
        r.watermark.checkpoint_hi = hi
    if lo is not None:
        r.watermark.checkpoint_lo = lo
    return r


def tx_end(reason):
    r = ls.ListTransactionsResponse()
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


IL = qo.QUERY_END_REASON_ITEM_LIMIT
CB = qo.QUERY_END_REASON_CHECKPOINT_BOUND
SL = qo.QUERY_END_REASON_SCAN_LIMIT
LT = qo.QUERY_END_REASON_LEDGER_TIP


# --- drain --------------------------------------------------------------------

def test_drain_resumes_across_pages():
    send = FakeSend({
        b"": [tx_item("A", b"c1", hi=10), tx_item("B", b"c2", hi=11),
              tx_item("C", b"c3", hi=12), tx_end(IL)],
        b"c3": [tx_item("D", b"c4", hi=13), tx_item("E", b"c5", hi=14), tx_end(CB)],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C"), ("tx", "D"), ("tx", "E")]
    assert len(r.pages) == 2
    assert r.terminal_reason == CB
    assert r.tiling_ok and r.watermark_ok
    assert send.calls == [b"", b"c3"]  # resumed from last cursor


def test_drain_detects_tiling_violation():
    send = FakeSend({
        b"": [tx_item("A", b"c1", hi=1), tx_item("B", b"c2", hi=2), tx_end(IL)],
        b"c2": [tx_item("B", b"c3", hi=3), tx_item("C", b"c4", hi=4), tx_end(CB)],  # B repeats
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.tiling_ok is False
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C")]  # dedup keeps first


def test_drain_detects_nonmonotonic_watermark():
    send = FakeSend({
        b"": [tx_item("A", b"c1", hi=10), tx_item("B", b"c2", hi=5), tx_end(CB)],  # hi went back
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.watermark_ok is False


def test_drain_standalone_watermark_advances_cursor():
    send = FakeSend({
        b"": [tx_wm(b"w1", hi=100), tx_end(SL)],   # no items, scan-progress only
        b"w1": [tx_item("Z", b"w2", hi=200), tx_end(CB)],
    })
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.ids == [("tx", "Z")]
    assert send.calls == [b"", b"w1"]


def test_drain_no_progress_breaks():
    # resume reason but cursor never advances -> must not loop forever
    send = FakeSend({b"": [tx_item("A", hi=1), tx_end(SL)]})  # no cursor
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A")]
    assert len(r.pages) == 1


def test_drain_caps_at_max_drain():
    send = FakeSend({
        b"": [tx_item("A", b"c1", hi=1), tx_item("B", b"c2", hi=2), tx_end(IL)],
        b"c2": [tx_item("C", b"c3", hi=3), tx_end(CB)],
    })
    r = H.drain(send, "ListTransactions", asc_req(), max_drain=2)
    assert r.capped is True
    assert len(r.ids) == 2


def test_drain_single_page_mode():
    send = FakeSend({b"": [tx_wm(b"c1", hi=5), tx_end(SL)]})
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
    good = [tx_item("A", b"c1", hi=1), tx_item("B", b"c2", hi=2), tx_end(CB)]

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
            return iter([tx_item("A", b"c1", hi=1), tx_item("B", b"c2", hi=2)])  # no end frame
        return iter([tx_item("C", b"c3", hi=3), tx_end(CB)])
    r = H.drain(send, "ListTransactions", asc_req())
    assert r.error is None
    assert r.ids == [("tx", "A"), ("tx", "B"), ("tx", "C")]
    assert calls["n"] == 2  # resumed after the truncated first stream


def test_drain_truncation_without_progress_errors():
    # truncated with no cursor advance -> cannot resume -> error (not a silent short count)
    def send(req):
        return iter([tx_item("A", hi=1)])  # no cursor, no QueryEnd
    r = H.drain(send, "ListTransactions", asc_req(), max_retries=2)
    assert r.error and "truncated" in r.error


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
    # small limit_items -> verify first page == limit + ITEM_LIMIT, not the full count
    rec = {"id": "c", "rpc": "ListTransactions",
           "request": {"options": {"limit_items": 5}},
           "oracle": {"kind": "exact_count", "expected_count": 47208,
                      "expected_end_reason": "QUERY_END_REASON_ITEM_LIMIT"}}
    good = _dr(["A", "B", "C", "D", "E"], pages=[H.PageMeta(5, IL)])
    assert H.check_oracle(rec, {"c": good}, 1000)[0] == "PASS"   # 5 != 47208 but probe passes
    short = _dr(["A", "B"], pages=[H.PageMeta(2, IL)])
    assert H.check_oracle(rec, {"c": short}, 1000)[0] == "FAIL"  # didn't honor limit
    wrong = _dr(["A", "B", "C", "D", "E"], pages=[H.PageMeta(5, CB)])
    assert H.check_oracle(rec, {"c": wrong}, 1000)[0] == "FAIL"  # wrong end_reason


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
