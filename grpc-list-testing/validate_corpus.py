# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Validate a generated corpus.jsonl independently of how it was built.

Checks (fail-fast, non-zero exit on any error):
  - every line is a well-formed record with the required envelope keys.
  - ids are unique.
  - `request` round-trips through corpus_builder's filter validation for its rpc
    (catches event-space predicate leaks, malformed DNF, etc.).
  - `end_checkpoint` present and > start; ranges within the frozen ceiling.
  - decomposition oracles reference component ids that exist in the corpus.
  - exact_count / degenerate oracles carry an integer expected_count.
Prints a coverage summary.
"""

from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path

import corpus_builder as b

DEFAULT_CEILING = 288_000_000
REQUIRED = {"id", "rpc", "request", "class", "oracle"}
CLASS_KEYS = {"dimension", "combinator", "selectivity_tier", "cost_class", "backend_scope"}
OPTIONS_KEYS = {"limit", "after", "before", "ordering"}


def validate(path: str) -> int:
    p = Path(path)
    records = [json.loads(line) for line in p.read_text().splitlines() if line.strip()]
    # ceiling from the sibling manifest (corpus.<net>.jsonl -> manifest.<net>.json)
    ceiling = DEFAULT_CEILING
    net = p.stem.split(".", 1)[1] if "." in p.stem else None
    mpath = p.with_name(f"manifest.{net}.json") if net else p.with_name("manifest.json")
    if mpath.exists():
        ceiling = json.loads(mpath.read_text()).get("cp_ceiling", DEFAULT_CEILING)
    errors: list[str] = []
    ids: set[str] = set()

    for i, r in enumerate(records):
        tag = r.get("id", f"<line {i}>")
        missing = REQUIRED - r.keys()
        if missing:
            errors.append(f"{tag}: missing keys {missing}")
            continue
        if r["id"] in ids:
            errors.append(f"{tag}: duplicate id")
        ids.add(r["id"])
        if r["rpc"] not in b.RPCS:
            errors.append(f"{tag}: bad rpc {r['rpc']}")
        if CLASS_KEYS - r["class"].keys():
            errors.append(f"{tag}: class missing {CLASS_KEYS - r['class'].keys()}")

        req = r["request"]
        end = req.get("end_checkpoint")
        start = req.get("start_checkpoint", 0)
        if end is None:
            errors.append(f"{tag}: request.end_checkpoint missing")
        else:
            if end > ceiling:
                errors.append(f"{tag}: end_checkpoint {end} > ceiling {ceiling}")
            if start >= end:
                errors.append(f"{tag}: start {start} >= end {end}")
        opts = req.get("options")
        if opts is not None:
            if not isinstance(opts, dict):
                errors.append(f"{tag}: request.options must be an object")
            else:
                unknown_options = opts.keys() - OPTIONS_KEYS
                if unknown_options:
                    errors.append(
                        f"{tag}: request.options has unknown keys {unknown_options}"
                    )
        # re-validate the filter against the rpc's allowed predicate space
        if "filter" in req and r["rpc"] in b.RPCS:
            try:
                b._validate_filter(r["rpc"], req["filter"])
            except ValueError as e:
                errors.append(f"{tag}: filter invalid for {r['rpc']}: {e}")

        oracle = r["oracle"]
        kind = oracle.get("kind")
        if kind in ("exact_count", "degenerate") and not isinstance(oracle.get("expected_count"), int):
            errors.append(f"{tag}: {kind} oracle missing integer expected_count")
        if kind == "decomposition":
            comps = oracle.get("components") or []
            if oracle.get("relation") not in ("union", "difference"):
                errors.append(f"{tag}: decomposition needs relation union|difference")
            if len(comps) < 2:
                errors.append(f"{tag}: decomposition needs >=2 components")

    # second pass: decomposition component ids must exist
    for r in records:
        if r.get("oracle", {}).get("kind") == "decomposition":
            for cid in r["oracle"].get("components", []):
                if cid not in ids:
                    errors.append(f"{r['id']}: decomposition component {cid!r} not in corpus")

    # ---- report ----
    print(f"records: {len(records)}")
    for axis in ("rpc",):
        print(f"by {axis}:", dict(Counter(r[axis] for r in records)))
    for axis in ("combinator", "selectivity_tier", "cost_class", "backend_scope", "dimension"):
        print(f"by {axis}:", dict(Counter(r["class"].get(axis) for r in records)))
    print("by oracle.kind:", dict(Counter(r["oracle"].get("kind") for r in records)))

    if errors:
        print(f"\nFAILED with {len(errors)} error(s):")
        for e in errors:
            print("  -", e)
        return 1
    print("\nOK: all records valid.")
    return 0


if __name__ == "__main__":
    sys.exit(validate(sys.argv[1] if len(sys.argv) > 1 else "corpus.jsonl"))
