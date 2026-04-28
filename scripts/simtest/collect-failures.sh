#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Usage: collect-failures.sh <log_dir>
#
# Emits unique "package::binary::test" lines (one per failed/timed-out test)
# pulled from both Phase 1 (seed-search NDJSON) and Phase 2/3 (nextest
# plaintext) outputs in <log_dir>.

set -euo pipefail
LOG_DIR="${1:?usage: collect-failures.sh <log_dir>}"

{
  if [ -f "$LOG_DIR/e2e/failures.ndjson" ]; then
    # Records that include `.package` line up with the nextest format used by
    # phases 2 & 3; those that don't (e.g. an explicit binary path) fall back
    # to "binary::test".
    jq -r '
      if .package then "\(.package)::\(.binary)::\(.test)"
      else "\(.binary)::\(.test)"
      end
    ' "$LOG_DIR/e2e/failures.ndjson"
  fi

  # Phase 2/3 still use nextest output: "FAIL [time] package::binary::test".
  # Globs are explicit (log-*, determinism-log) so we do not pick up Phase 1's
  # per-job logs under $LOG_DIR/e2e/ and try to parse them as nextest output.
  for f in "$LOG_DIR"/log-* "$LOG_DIR"/determinism-log; do
    [ -f "$f" ] || continue
    sed 's/\x1b\[[0-9;]*m//g' "$f" \
      | grep -E '^[[:space:]]*(FAIL|TIMEOUT)[[:space:]]+\[' \
      | sed -E 's/^[[:space:]]*(FAIL|TIMEOUT)[[:space:]]+\[[^]]+\][[:space:]]+//'
  done
} | grep -v '^$' | sort -u
