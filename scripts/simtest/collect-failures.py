#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Collect and render simtest-run failures from a log directory.

Two output formats:

  --format=list      Unique `package::binary::test` lines (one per failing
                     test), sorted and deduped across both Phase 1
                     (failures.ndjson written by seed-search.py) and
                     Phase 2/3 (nextest plaintext logs). Used by the
                     workflow's slack-notification step.

  --format=detailed  Human-readable failure report intended for operators
                     reading simtest_stdout.log. Phase 1 failures are
                     grouped by (binary, test) with one log tail per group
                     (panic + nextest result line); Phase 2/3 plaintext
                     logs that contain failures are dumped in full.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import json
import os
import re
import sys
from typing import List, Optional


# --------------------------------------------------------------------------
# Phase 1: failures.ndjson written by seed-search.py
# --------------------------------------------------------------------------

@dataclasses.dataclass
class Phase1Failure:
    status: str             # "FAIL" | "TIMEOUT"
    binary: str             # e.g. "consensus_dag_tests"
    test: str               # e.g. "consensus_dag_tests::test_randomized_dag_..."
    seed: str               # e.g. "1777658876"
    package: Optional[str]  # e.g. "consensus-simtests"; None for explicit-path tests
    log: Optional[str]      # filename in <log_dir>/e2e/

    def qualified_name(self) -> str:
        if self.package:
            return f"{self.package}::{self.binary}::{self.test}"
        return f"{self.binary}::{self.test}"

    @classmethod
    def from_record(cls, r: dict) -> "Phase1Failure":
        return cls(
            status=r["status"],
            binary=r["binary"],
            test=r["test"],
            seed=r["seed"],
            package=r.get("package"),
            log=r.get("log"),
        )


def load_phase1_failures(log_dir: str) -> List[Phase1Failure]:
    path = os.path.join(log_dir, "e2e", "failures.ndjson")
    if not os.path.exists(path):
        return []
    out: List[Phase1Failure] = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            out.append(Phase1Failure.from_record(json.loads(line)))
    return out


# --------------------------------------------------------------------------
# Phase 2/3: plaintext nextest logs (log-* per stress iteration plus
# determinism-log). Phase 1's per-job logs live under e2e/ and are
# intentionally NOT scanned here.
#
# TODO: this regex misses signal-based terminations. nextest also emits
# status lines like "SIGABRT [time] pkg::bin::test" (and SIGSEGV, SIGBUS,
# SIGKILL, SIGTRAP, SIGFPE, SIGSYS, plus LEAK), none of which match
# FAIL|TIMEOUT. simtest-run.sh's failure-detection grep has the same gap
# — keep them in sync when this is fixed.
# --------------------------------------------------------------------------

_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
_NEXTEST_FAIL_RE = re.compile(
    r"^[ \t]*(?:FAIL|TIMEOUT)[ \t]+\[[^\]]+\][ \t]+(.+?)[ \t]*$"
)


def phase23_log_paths(log_dir: str) -> List[str]:
    out = []
    if not os.path.isdir(log_dir):
        return out
    for name in os.listdir(log_dir):
        if name.startswith("log-") or name == "determinism-log":
            full = os.path.join(log_dir, name)
            if os.path.isfile(full):
                out.append(full)
    return sorted(out)


def phase23_failing_tests(path: str) -> List[str]:
    """Extract `package::binary::test` from FAIL/TIMEOUT lines in one nextest log."""
    failures: List[str] = []
    with open(path, errors="replace") as f:
        for line in f:
            m = _NEXTEST_FAIL_RE.match(_ANSI_RE.sub("", line))
            if m:
                failures.append(m.group(1))
    return failures


# --------------------------------------------------------------------------
# Renderers
# --------------------------------------------------------------------------

def render_list(log_dir: str) -> None:
    seen = set()
    for f in load_phase1_failures(log_dir):
        seen.add(f.qualified_name())
    for path in phase23_log_paths(log_dir):
        seen.update(phase23_failing_tests(path))
    for line in sorted(seen):
        print(line)


def render_detailed(log_dir: str, max_tests: int, tail_lines: int) -> None:
    failures = load_phase1_failures(log_dir)
    if failures:
        _render_phase1_detailed(log_dir, failures, max_tests, tail_lines)

    for path in phase23_log_paths(log_dir):
        if not phase23_failing_tests(path):
            continue
        print()
        print("------------------------------")
        print(f"Phase 2/3 nextest log: {path}")
        print("------------------------------")
        with open(path, errors="replace") as f:
            sys.stdout.write(f.read())


def _render_phase1_detailed(log_dir, failures, max_tests, tail_lines):
    groups = collections.OrderedDict()
    for fail in failures:
        groups.setdefault((fail.binary, fail.test), []).append(fail)

    shown = 0
    for (binary, test), recs in groups.items():
        if shown >= max_tests:
            break
        shown += 1
        seeds = sorted({f.seed for f in recs})
        statuses = sorted({f.status for f in recs})
        seed_preview = ", ".join(seeds[:10]) + (", ..." if len(seeds) > 10 else "")
        print()
        print("------------------------------")
        print(f"{'/'.join(statuses)} {binary}::{test} "
              f"({len(recs)} seed(s): {seed_preview})")

        sample = recs[0]
        if not sample.log:
            continue
        log_path = os.path.join(log_dir, "e2e", sample.log)
        if not os.path.exists(log_path):
            print(f"--- log file missing: {log_path}")
            continue
        print(f"--- last {tail_lines} lines of seed={sample.seed} log ({sample.log}):")
        try:
            with open(log_path, errors="replace") as f:
                tail = f.readlines()[-tail_lines:]
        except OSError as e:
            print(f"    (could not read log: {e!r})")
            continue
        for ln in tail:
            print("    " + ln.rstrip())

    remaining = max(0, len(groups) - shown)
    if remaining > 0:
        print()
        print(f"... and {remaining} more distinct failing test(s); showing first {shown}.")
    print()
    print(f"Phase 1 failure summary: {len(failures)} record(s) "
          f"across {len(groups)} distinct test(s).")


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("log_dir", help="Path to the simtest log directory.")
    parser.add_argument("--format", choices=("list", "detailed"), default="list",
                        help="Output format (default: list).")
    parser.add_argument(
        "--max-tests", type=int,
        default=int(os.environ.get("SIMTEST_FAILURE_REPORT_MAX_TESTS", "50")),
        help="Cap on distinct (binary, test) groups in detailed Phase 1 output.",
    )
    parser.add_argument(
        "--tail-lines", type=int,
        default=int(os.environ.get("SIMTEST_FAILURE_REPORT_TAIL_LINES", "100")),
        help="Lines of per-job log to tail per group in detailed Phase 1 output.",
    )
    args = parser.parse_args()

    if args.format == "list":
        render_list(args.log_dir)
    else:
        render_detailed(args.log_dir, args.max_tests, args.tail_lines)


if __name__ == "__main__":
    main()
