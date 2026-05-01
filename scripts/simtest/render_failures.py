#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Render seed-search.py's failures.ndjson into operator-friendly text.

Two output formats:

  --format=list      One unique `package::binary::test` line per failing test
                     (sorted, deduped across seeds). Used by the workflow's
                     slack-notification step alongside Phase 2/3 nextest
                     plaintext.

  --format=detailed  Group records by `(binary, test)`, list each group's
                     failing seeds, and tail one of the per-job log files
                     (panic + nextest result line). Caps at MAX_TESTS
                     distinct tests so a totally-broken build doesn't dump
                     gigabytes. Requires --log-dir to locate per-job logs.
"""

import argparse
import collections
import json
import os
import sys


def load_records(source):
    if source == "-":
        stream = sys.stdin
    else:
        stream = open(source)
    try:
        for line in stream:
            line = line.strip()
            if not line:
                continue
            yield json.loads(line)
    finally:
        if source != "-":
            stream.close()


def render_list(records):
    seen = set()
    for r in records:
        pkg = r.get("package")
        if pkg:
            line = pkg + "::" + r["binary"] + "::" + r["test"]
        else:
            line = r["binary"] + "::" + r["test"]
        seen.add(line)
    for line in sorted(seen):
        print(line)


def render_detailed(records, log_dir, max_tests, tail_lines):
    records = list(records)
    groups = collections.OrderedDict()
    for r in records:
        groups.setdefault((r["binary"], r["test"]), []).append(r)

    shown = 0
    for (binary, test), recs in groups.items():
        if shown >= max_tests:
            break
        shown += 1
        seeds = sorted({r["seed"] for r in recs})
        statuses = sorted({r["status"] for r in recs})
        seed_preview = ", ".join(seeds[:10]) + (", ..." if len(seeds) > 10 else "")
        print()
        print("------------------------------")
        print(f"{'/'.join(statuses)} {binary}::{test} "
              f"({len(recs)} seed(s): {seed_preview})")

        sample = recs[0]
        log_name = sample.get("log") or ""
        if not log_name:
            continue
        log_path = os.path.join(log_dir, "e2e", log_name)
        if not os.path.exists(log_path):
            print(f"--- log file missing: {log_path}")
            continue
        print(f"--- last {tail_lines} lines of seed={sample['seed']} log ({log_name}):")
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
        print(f"... and {remaining} more distinct failing test(s); "
              f"showing first {shown}.")
    print()
    print(f"Phase 1 failure summary: {len(records)} record(s) "
          f"across {len(groups)} distinct test(s).")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("list", "detailed"), default="list")
    parser.add_argument("--log-dir",
                        help="Path containing e2e/<log files>; required for --format=detailed.")
    parser.add_argument("--max-tests", type=int,
                        default=int(os.environ.get("SIMTEST_FAILURE_REPORT_MAX_TESTS", "50")),
                        help="Cap on distinct (binary, test) groups rendered in detailed mode.")
    parser.add_argument("--tail-lines", type=int,
                        default=int(os.environ.get("SIMTEST_FAILURE_REPORT_TAIL_LINES", "100")),
                        help="Lines of per-job log to tail per group in detailed mode.")
    parser.add_argument("ndjson", nargs="?", default="-",
                        help="Path to failures.ndjson, or '-' for stdin (default).")
    args = parser.parse_args()

    records = load_records(args.ndjson)
    if args.format == "list":
        render_list(records)
    else:
        if not args.log_dir:
            parser.error("--log-dir is required for --format=detailed")
        render_detailed(records, args.log_dir, args.max_tests, args.tail_lines)


if __name__ == "__main__":
    main()
