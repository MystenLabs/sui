# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Builder for v2alpha gRPC List API test-case records (the corpus).

Each record is a self-describing envelope wrapping a **verbatim protojson
request** that maps 1:1 onto `sui.rpc.v2alpha.List{Transactions,Events,
Checkpoints}Request` — exactly what k6 puts on the wire, and what the Rust
correctness harness parses into the same proto type. One source of truth, no
bespoke filter parser, no load-vs-correctness drift.

Field names are **snake_case**, matching the proto (rev 43c5bc1) and the casing
proven to work with k6's protojson in the streaming/pagination spikes
(`read_mask`, `move_call`, `affected_object{object_id}`, ...).

Wire contract (verified against the pinned proto):
  ListXRequest {
    read_mask:        google.protobuf.FieldMask   # protojson: comma-joined path string; omit -> server default
    start_checkpoint: uint64                       # inclusive
    end_checkpoint:   uint64                       # EXCLUSIVE
    filter:           TransactionFilter|EventFilter# DNF; omit -> match all
    options:          QueryOptions { limit_items, after, before, ordering }
  }
  DNF: filter.terms[] (OR) of term.literals[] (AND) of {include|exclude: predicate}.
  Predicate oneofs (tx-space): sender, affected_address, affected_object,
    move_call, emit_module, event_type, event_stream_head, package_write.
  Predicate oneofs (event-space, strict subset): sender, emit_module,
    event_type, event_stream_head.
"""

from __future__ import annotations

import dataclasses
import json
from typing import Any, Iterable, Optional

# --- RPCs and their filter space -------------------------------------------------

RPCS = ("ListTransactions", "ListEvents", "ListCheckpoints")

# Predicate names valid for each RPC's filter (EventFilter is a strict subset).
# ListCheckpoints uses a TransactionFilter (a checkpoint matches if any tx in it
# satisfies the filter), so it takes the full tx-space predicate set.
_TX_PREDICATES = frozenset(
    {
        "sender",
        "affected_address",
        "affected_object",
        "move_call",
        "emit_module",
        "event_type",
        "event_stream_head",
        "package_write",
    }
)
_EVENT_PREDICATES = frozenset({"sender", "emit_module", "event_type", "event_stream_head"})
_PREDICATES_FOR_RPC = {
    "ListTransactions": _TX_PREDICATES,
    "ListCheckpoints": _TX_PREDICATES,
    "ListEvents": _EVENT_PREDICATES,
}

ORDER_ASC = "ORDERING_ASCENDING"
ORDER_DESC = "ORDERING_DESCENDING"

# Whole-message heavy read masks (full object/JSON/package resolution). Paths are
# validated server-side against the item BODY message: ExecutedTransaction (tx),
# Checkpoint (cp), Event (ev) -- NOT the alpha *Item wrapper.
HEAVY_READ_MASK = {
    "ListTransactions": "digest,transaction,signatures,effects,events,checkpoint,timestamp,balanceChanges,objects",
    "ListCheckpoints": "sequenceNumber,digest,summary,signature,contents,transactions,objects",
    "ListEvents": "packageId,module,sender,eventType,contents,json",
}
# Cheapest valid mask per RPC: the body's identity/digest field (matches each
# RPC's server default family). For events the per-item identity
# (transaction_digest, event_index) is on the EventItem and returned regardless.
# protojson FieldMask strings are camelCase; the wire form is snake_case.
CHEAP_READ_MASK = {
    "ListTransactions": "digest",
    "ListCheckpoints": "sequenceNumber",
    "ListEvents": "eventType",
}


# --- address normalization -------------------------------------------------------


def addr32(a: str) -> str:
    """Normalize a hex address/object-id to 0x + 64 hex chars.

    The server rejects short forms (`0x3`) with `invalid address`; every
    sender/object/stream-id literal must be 32-byte zero-padded.
    """
    h = a.lower()
    if h.startswith("0x"):
        h = h[2:]
    if not h or any(c not in "0123456789abcdef" for c in h):
        raise ValueError(f"not hex: {a!r}")
    if len(h) > 64:
        raise ValueError(f"address too long ({len(h)} hex chars): {a!r}")
    return "0x" + h.rjust(64, "0")


# --- predicate constructors (return a *Predicate protojson dict) -----------------


def sender(address: str) -> dict:
    return {"sender": {"address": addr32(address)}}


def affected_address(address: str) -> dict:
    return {"affected_address": {"address": addr32(address)}}


def affected_object(object_id: str) -> dict:
    return {"affected_object": {"object_id": addr32(object_id)}}


def move_call(path: str) -> dict:
    """`package[::module[::function]]` (specificity decides cost)."""
    return {"move_call": {"function": _normalize_move_path(path, max_parts=3)}}


def emit_module(path: str) -> dict:
    """`package[::module]`."""
    return {"emit_module": {"module": _normalize_move_path(path, max_parts=2)}}


def event_type(type_str: str) -> dict:
    """`address[::module[::Name[<type_params>]]]`."""
    return {"event_type": {"type": _normalize_event_type(type_str)}}


def event_stream_head(stream_id: str) -> dict:
    return {"event_stream_head": {"stream_id": addr32(stream_id)}}


def package_write() -> dict:
    return {"package_write": {}}


def _predicate_name(predicate: dict) -> str:
    (name,) = predicate.keys()
    return name


def _normalize_move_path(path: str, *, max_parts: int) -> str:
    parts = path.split("::")
    if not (1 <= len(parts) <= max_parts):
        raise ValueError(f"move path {path!r}: expected 1..{max_parts} '::'-parts")
    parts[0] = addr32(parts[0])  # package address must be 32-byte padded
    return "::".join(parts)


def _normalize_event_type(type_str: str) -> str:
    head = type_str.split("<", 1)[0]
    parts = head.split("::")
    if not (1 <= len(parts) <= 3):
        raise ValueError(f"event type {type_str!r}: expected address[::module[::Name]]")
    addr = addr32(parts[0])
    rest = type_str[len(parts[0]):]  # keep ::module::Name<...> tail verbatim
    return addr + rest


# --- DNF combinators -------------------------------------------------------------


def lit(predicate: dict, *, negate: bool = False) -> dict:
    return {("exclude" if negate else "include"): predicate}


def term(*literals: dict) -> dict:
    if not literals:
        raise ValueError("term needs at least one literal")
    return {"literals": list(literals)}


def dnf(*terms: dict) -> dict:
    if not terms:
        raise ValueError("filter needs at least one term")
    return {"terms": list(terms)}


# Convenience filters for the common shapes.
def f_single(predicate: dict, *, negate: bool = False) -> dict:
    """Single literal (or single unanchored negation when negate=True)."""
    return dnf(term(lit(predicate, negate=negate)))


def f_and(*predicates: dict) -> dict:
    """One term, all predicates ANDed (each included)."""
    return dnf(term(*[lit(p) for p in predicates]))


def f_and_not(include: dict, *excludes: dict) -> dict:
    """`include AND NOT e1 AND NOT e2 ...` (anchored negation)."""
    return dnf(term(lit(include), *[lit(e, negate=True) for e in excludes]))


def f_or(*predicates: dict) -> dict:
    """Disjunction: one term per predicate."""
    return dnf(*[term(lit(p)) for p in predicates])


# --- options + request -----------------------------------------------------------


def options(
    *,
    limit_items: Optional[int] = None,
    ordering: str = ORDER_ASC,
    after: Optional[str] = None,
    before: Optional[str] = None,
) -> Optional[dict]:
    o: dict = {}
    if limit_items is not None:
        o["limit_items"] = int(limit_items)
    if ordering == ORDER_DESC:
        o["ordering"] = ORDER_DESC  # ascending is proto3 default 0 -> omit
    elif ordering != ORDER_ASC:
        raise ValueError(f"bad ordering {ordering!r}")
    if after is not None:
        o["after"] = after  # opaque base64 cursor (rarely used; Model-A seeds by checkpoint)
    if before is not None:
        o["before"] = before
    return o or None


def request(
    rpc: str,
    *,
    end_checkpoint: int,
    start_checkpoint: Optional[int] = None,
    filter: Optional[dict] = None,  # noqa: A002 - matches proto field name
    read_mask: Optional[Any] = None,  # str | list[str] | None(=server default)
    opts: Optional[dict] = None,
) -> dict:
    if rpc not in RPCS:
        raise ValueError(f"unknown rpc {rpc!r}")
    if filter is not None:
        _validate_filter(rpc, filter)
    req: dict = {"end_checkpoint": int(end_checkpoint)}
    if start_checkpoint is not None:
        req["start_checkpoint"] = int(start_checkpoint)
    if read_mask is not None:
        req["read_mask"] = read_mask if isinstance(read_mask, str) else ",".join(read_mask)
    if filter is not None:
        req["filter"] = filter
    if opts:
        req["options"] = opts
    return req


def _validate_filter(rpc: str, filt: dict) -> None:
    allowed = _PREDICATES_FOR_RPC[rpc]
    terms = filt.get("terms")
    if not terms:
        raise ValueError("filter has no terms")
    for t in terms:
        lits = t.get("literals")
        if not lits:
            raise ValueError("term has no literals")
        for literal in lits:
            polarity = "include" if "include" in literal else "exclude"
            name = _predicate_name(literal[polarity])
            if name not in allowed:
                raise ValueError(
                    f"predicate {name!r} not valid for {rpc} "
                    f"(allowed: {sorted(allowed)})"
                )


# --- record envelope -------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Klass:
    dimension: str
    combinator: str  # single | and | or | not | dnf
    selectivity_tier: str  # dense_everywhere | recent_only | sparse | bursty | empty_degenerate | mixed | na
    cost_class: str  # cheap | expensive | adversarial
    backend_scope: str  # shared | archival_only
    specificity: Optional[str] = None  # package | module | function | name | generic | na


@dataclasses.dataclass(frozen=True)
class Oracle:
    kind: str  # exact_count | decomposition | membership | degenerate | none
    expected_count: Optional[int] = None
    expected_end_reason: Optional[str] = None
    sql_ref: Optional[str] = None
    relation: Optional[str] = None  # union | difference (for kind=decomposition)
    components: tuple[str, ...] = ()


@dataclasses.dataclass(frozen=True)
class Case:
    id: str
    rpc: str
    request: dict
    klass: Klass
    oracle: Oracle

    def to_record(self) -> dict:
        return {
            "id": self.id,
            "rpc": self.rpc,
            "request": self.request,
            "class": _drop_none(dataclasses.asdict(self.klass)),
            "oracle": _drop_none(dataclasses.asdict(self.oracle)),
        }

    def to_json(self) -> str:
        return json.dumps(self.to_record(), separators=(",", ":"), sort_keys=False)


def _drop_none(d: dict) -> dict:
    return {k: v for k, v in d.items() if v is not None and v != ()}


def write_corpus(cases: Iterable[Case], path: str) -> int:
    ids: set[str] = set()
    n = 0
    with open(path, "w") as fh:
        for c in cases:
            if c.id in ids:
                raise ValueError(f"duplicate case id {c.id!r}")
            ids.add(c.id)
            fh.write(c.to_json() + "\n")
            n += 1
    return n


if __name__ == "__main__":
    # Smoke: print a couple of records.
    demo = [
        Case(
            id="tx.sender.single.demo",
            rpc="ListTransactions",
            request=request(
                "ListTransactions",
                start_checkpoint=278_000_000,
                end_checkpoint=288_000_000,
                filter=f_single(sender("0xabc")),
                read_mask="transaction.digest",
                opts=options(limit_items=1000),
            ),
            klass=Klass("sender", "single", "sparse", "cheap", "shared", specificity="na"),
            oracle=Oracle("exact_count", expected_count=42, sql_ref="queries/sender.sql#x"),
        ),
        Case(
            id="ev.sender.or.demo",
            rpc="ListEvents",
            request=request(
                "ListEvents",
                end_checkpoint=288_000_000,
                filter=f_or(sender("0x1"), sender("0x2")),
                opts=options(ordering=ORDER_DESC),
            ),
            klass=Klass("sender", "or", "mixed", "expensive", "shared"),
            oracle=Oracle("decomposition", relation="union",
                          components=("ev.sender.a", "ev.sender.b")),
        ),
    ]
    for c in demo:
        print(c.to_json())
