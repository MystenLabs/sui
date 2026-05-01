#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Usage: collect-failures.sh [--detailed] <log_dir>
#
# Default mode emits unique "package::binary::test" lines (one per
# failed/timed-out test) pulled from both Phase 1 (seed-search NDJSON) and
# Phase 2/3 (nextest plaintext) outputs in <log_dir>. Used by the workflow's
# slack-notification step.
#
# --detailed mode emits a richer, human-readable failure report intended for
# operators reading simtest_stdout.log: Phase 1 failures grouped by
# (binary, test) with a tail of one log per group, plus full nextest
# plaintext for any failing Phase 2/3 stress/determinism logs.

set -uo pipefail

DETAILED=0
if [ "${1:-}" = "--detailed" ]; then
  DETAILED=1
  shift
fi
LOG_DIR="${1:?usage: collect-failures.sh [--detailed] <log_dir>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ "$DETAILED" -eq 1 ]; then
  if [ -s "$LOG_DIR/e2e/failures.ndjson" ]; then
    "$SCRIPT_DIR/render_failures.py" --format=detailed --log-dir "$LOG_DIR" \
      "$LOG_DIR/e2e/failures.ndjson"
  fi

  for f in "$LOG_DIR"/log-* "$LOG_DIR/determinism-log"; do
    [ -f "$f" ] || continue
    if grep -EqHn 'TIMEOUT|FAIL' "$f" 2>/dev/null; then
      echo
      echo "------------------------------"
      echo "Phase 2/3 nextest log: $f"
      echo "------------------------------"
      cat "$f"
    fi
  done
  exit 0
fi

# Default: emit unique package::binary::test lines, sorted/deduped.
{
  if [ -f "$LOG_DIR/e2e/failures.ndjson" ]; then
    "$SCRIPT_DIR/render_failures.py" --format=list "$LOG_DIR/e2e/failures.ndjson"
  fi

  # Phase 2/3 use nextest output: "FAIL [time] package::binary::test".
  # Globs are explicit (log-*, determinism-log) so we do not pick up Phase 1's
  # per-job logs under $LOG_DIR/e2e/ and try to parse them as nextest output.
  #
  # TODO: this regex misses signal-based terminations. nextest also emits
  # status lines like "SIGABRT [time] pkg::bin::test" (and SIGSEGV, SIGBUS,
  # SIGKILL, SIGTRAP, SIGFPE, SIGSYS, plus LEAK), none of which match
  # FAIL|TIMEOUT. simtest-run.sh's failure-detection grep has the same gap
  # — keep them in sync when this is fixed.
  for f in "$LOG_DIR"/log-* "$LOG_DIR/determinism-log"; do
    [ -f "$f" ] || continue
    sed 's/\x1b\[[0-9;]*m//g' "$f" \
      | { grep -E '^[[:space:]]*(FAIL|TIMEOUT)[[:space:]]+\[' || true; } \
      | sed -E 's/^[[:space:]]*(FAIL|TIMEOUT)[[:space:]]+\[[^]]+\][[:space:]]+//'
  done
} | { grep -v '^$' || true; } | sort -u
