#!/usr/bin/env bash
set -euo pipefail

source "${TEST_SANDBOX_DIR}/common.sh"

print_section "advance-clock"

stdout=""
stderr=""
if ! run_cmd stdout stderr "$SUI_FORKING_BIN" advance-clock --server-url "$FORKING_SERVER_URL" --ms 7000; then
  echo "$stderr" >&2
  echo "advance-clock command failed" >&2
  exit 1
fi

echo "advance_clock_succeeded=true"
