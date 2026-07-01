# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for corpus_builder: assert protojson matches the verified wire shape."""

import json

import pytest

import corpus_builder as b


# --- address normalization ---


def test_addr32_pads_short():
    assert b.addr32("0x3") == "0x" + "0" * 63 + "3"
    assert b.addr32("3") == "0x" + "0" * 63 + "3"


def test_addr32_keeps_full():
    full = "0x" + "a" * 64
    assert b.addr32(full) == full


def test_addr32_rejects_bad():
    with pytest.raises(ValueError):
        b.addr32("0xZZ")
    with pytest.raises(ValueError):
        b.addr32("0x" + "a" * 65)


# --- predicate shapes (snake_case, wrapper messages) ---


def test_sender_shape():
    assert b.sender("0x3") == {"sender": {"address": "0x" + "0" * 63 + "3"}}


def test_affected_object_uses_object_id_key():
    assert b.affected_object("0x5") == {"affected_object": {"object_id": "0x" + "0" * 63 + "5"}}


def test_move_call_pads_package_keeps_path():
    p = b.move_call("0x2::coin::mint")
    assert p == {"move_call": {"function": "0x" + "0" * 63 + "2" + "::coin::mint"}}


def test_emit_module_max_two_parts():
    assert b.emit_module("0x2::event")["emit_module"]["module"].endswith("2::event")
    with pytest.raises(ValueError):
        b.emit_module("0x2::event::extra")


def test_event_type_with_generics_preserved():
    t = b.event_type("0x2::coin::CoinCreated<0x2::sui::SUI>")
    s = t["event_type"]["type"]
    assert s.endswith("::coin::CoinCreated<0x2::sui::SUI>")
    assert s.startswith("0x" + "0" * 63 + "2::")


def test_package_write_is_empty_message():
    assert b.package_write() == {"package_write": {}}


# --- DNF combinators match the verified wire example ---


def test_single_literal_matches_spike_example():
    # §2.6 gotcha: {"terms":[{"literals":[{"include":{"sender":{"address": ...}}}]}]}
    full = "0x" + "0" * 63 + "a"
    assert b.f_single(b.sender("0xa")) == {
        "terms": [{"literals": [{"include": {"sender": {"address": full}}}]}]
    }


def test_unanchored_negation_exclude_only_term():
    f = b.f_single(b.sender("0xa"), negate=True)
    assert f["terms"][0]["literals"][0] == {"exclude": {"sender": {"address": "0x" + "0" * 63 + "a"}}}


def test_and_is_one_term_many_includes():
    f = b.f_and(b.sender("0xa"), b.move_call("0x2::m::f"))
    assert len(f["terms"]) == 1
    assert len(f["terms"][0]["literals"]) == 2
    assert all("include" in lit for lit in f["terms"][0]["literals"])


def test_and_not_anchored():
    f = b.f_and_not(b.sender("0xa"), b.move_call("0x2::m::f"))
    lits = f["terms"][0]["literals"]
    assert "include" in lits[0] and "exclude" in lits[1]


def test_or_is_many_terms():
    f = b.f_or(b.sender("0xa"), b.sender("0xb"))
    assert len(f["terms"]) == 2
    assert all(len(t["literals"]) == 1 for t in f["terms"])


# --- request assembly ---


def test_request_omits_ascending_ordering():
    req = b.request("ListTransactions", end_checkpoint=288_000_000,
                    opts=b.options(limit_items=10, ordering=b.ORDER_ASC))
    assert req["options"] == {"limit_items": 10}
    assert "start_checkpoint" not in req


def test_request_emits_descending_ordering():
    req = b.request("ListTransactions", end_checkpoint=1,
                    opts=b.options(ordering=b.ORDER_DESC))
    assert req["options"]["ordering"] == "ORDERING_DESCENDING"


def test_request_read_mask_list_joined():
    req = b.request("ListTransactions", end_checkpoint=1,
                    read_mask=["transaction.digest", "transaction.effects"])
    assert req["read_mask"] == "transaction.digest,transaction.effects"


def test_request_unfiltered_has_no_filter_key():
    req = b.request("ListCheckpoints", end_checkpoint=1)
    assert "filter" not in req


# --- event-space predicate validation (the generation landmine) ---


def test_event_rpc_rejects_tx_only_predicate():
    with pytest.raises(ValueError):
        b.request("ListEvents", end_checkpoint=1, filter=b.f_single(b.move_call("0x2::m::f")))
    with pytest.raises(ValueError):
        b.request("ListEvents", end_checkpoint=1, filter=b.f_single(b.affected_object("0x9")))


def test_event_rpc_accepts_event_space_predicate():
    b.request("ListEvents", end_checkpoint=1, filter=b.f_single(b.emit_module("0x2::event")))


def test_checkpoints_accept_tx_predicates():
    b.request("ListCheckpoints", end_checkpoint=1, filter=b.f_single(b.move_call("0x2::m::f")))


# --- envelope + serialization ---


def test_record_envelope_and_json_roundtrip():
    c = b.Case(
        id="tx.sender.single.x",
        rpc="ListTransactions",
        request=b.request("ListTransactions", end_checkpoint=288_000_000,
                          filter=b.f_single(b.sender("0xa"))),
        klass=b.Klass("sender", "single", "sparse", "cheap", "shared", specificity="na"),
        oracle=b.Oracle("exact_count", expected_count=7, sql_ref="q.sql#h"),
    )
    rec = json.loads(c.to_json())
    assert rec["id"] == "tx.sender.single.x"
    assert rec["rpc"] == "ListTransactions"
    assert rec["class"]["dimension"] == "sender"
    assert rec["oracle"]["kind"] == "exact_count"
    assert rec["oracle"]["expected_count"] == 7
    # empty components tuple dropped
    assert "components" not in rec["oracle"]


def test_write_corpus_rejects_duplicate_ids(tmp_path):
    c = b.Case("dup", "ListTransactions",
               b.request("ListTransactions", end_checkpoint=1),
               b.Klass("unfiltered", "single", "na", "cheap", "shared"),
               b.Oracle("none"))
    out = tmp_path / "c.jsonl"
    with pytest.raises(ValueError):
        b.write_corpus([c, c], str(out))


def test_decomposition_oracle_serializes_components():
    o = b.Oracle("decomposition", relation="union", components=("a", "b"))
    c = b.Case("or.x", "ListTransactions",
               b.request("ListTransactions", end_checkpoint=1,
                         filter=b.f_or(b.sender("0xa"), b.sender("0xb"))),
               b.Klass("sender", "or", "mixed", "expensive", "shared"),
               o)
    rec = json.loads(c.to_json())
    assert rec["oracle"]["relation"] == "union"
    assert rec["oracle"]["components"] == ["a", "b"]
