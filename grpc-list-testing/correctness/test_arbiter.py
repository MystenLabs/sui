# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Offline unit tests for arbiter.py's filter evaluator (synthetic tx dicts)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sui_pb"))
import arbiter as A

P2 = "0x0000000000000000000000000000000000000000000000000000000000000002"
OBJ = "0x0000000000000000000000000000000000000000000000000000000000000006"
ADDR = "0x00000000000000000000000000000000000000000000000000000000000000ab"


def tx(sender="0xab", calls=(), events=(), objs=()):
    return {
        "transaction": {"sender": sender, "kind": {"programmableTransaction": {"commands": [
            {"moveCall": {"package": p, "module": m, "function": f}} for (p, m, f) in calls]}}},
        "events": {"events": [{"packageId": p, "module": m, "eventType": t} for (p, m, t) in events]},
        "effects": {"changedObjects": [{"objectId": o} for o in objs]},
    }


def inc(pred):
    return {"terms": [{"literals": [pred]}]}


def test_sender():
    t = tx(sender="0xab")
    assert A.filter_matches(inc({"sender": {"address": ADDR}}), t) is True
    assert A.filter_matches(inc({"sender": {"address": P2}}), t) is False


def test_move_call_specificity():
    t = tx(calls=[(P2, "coin", "destroy_zero")])
    assert A.filter_matches(inc({"move_call": {"function": "0x2"}}), t) is True            # package
    assert A.filter_matches(inc({"move_call": {"function": "0x2::coin"}}), t) is True       # module
    assert A.filter_matches(inc({"move_call": {"function": "0x2::coin::destroy_zero"}}), t) is True
    assert A.filter_matches(inc({"move_call": {"function": "0x2::coin::mint"}}), t) is False
    assert A.filter_matches(inc({"move_call": {"function": "0x3"}}), t) is False


def test_affected_object():
    t = tx(objs=[OBJ, "0x07"])
    assert A.filter_matches(inc({"affected_object": {"object_id": "0x6"}}), t) is True
    assert A.filter_matches(inc({"affected_object": {"object_id": "0x9"}}), t) is False


def test_emit_module_and_event_type():
    t = tx(events=[(P2, "clock", f"{P2}::clock::Tick")])
    assert A.filter_matches(inc({"emit_module": {"module": "0x2::clock"}}), t) is True
    assert A.filter_matches(inc({"emit_module": {"module": "0x2::coin"}}), t) is False
    assert A.filter_matches(inc({"event_type": {"event_type": "0x2::clock::Tick"}}), t) is True
    assert A.filter_matches(inc({"event_type": {"event_type": "0x2::clock"}}), t) is True
    assert A.filter_matches(inc({"event_type": {"event_type": "0x2::coin::X"}}), t) is False


def test_dnf_or_and_negated():
    t = tx(sender="0xab", calls=[(P2, "coin", "destroy_zero")])
    f_or = {
        "terms": [
            {"literals": [{"sender": {"address": P2}}]},
            {"literals": [{"move_call": {"function": "0x2::coin::destroy_zero"}}]},
        ],
    }
    assert A.filter_matches(f_or, t) is True

    f_and = {
        "terms": [{
            "literals": [
                {"sender": {"address": ADDR}},
                {"move_call": {"function": "0x2::coin::mint"}},
            ],
        }],
    }
    assert A.filter_matches(f_and, t) is False

    f_not = {
        "terms": [{
            "literals": [
                {"sender": {"address": ADDR}},
                {"move_call": {"function": "0x2::coin::mint"}, "negated": True},
            ],
        }],
    }
    assert A.filter_matches(f_not, t) is True


def test_unsupported_predicate_returns_none():
    t = tx(sender="0xab")
    assert A.filter_matches(inc({"affected_address": {"address": ADDR}}), t) is None
    assert A.filter_matches(inc({"package_write": {}}), t) is None
    negated = {
        "terms": [{
            "literals": [{
                "affected_address": {"address": ADDR},
                "negated": True,
            }],
        }],
    }
    assert A.filter_matches(negated, t) is None


def test_norm_addr():
    assert A.norm_addr("0x2") == P2
    assert A.norm_addr("0X0000000000000000000000000000000000000000000000000000000000000002") == P2
    assert A.norm_addr(None) is None
