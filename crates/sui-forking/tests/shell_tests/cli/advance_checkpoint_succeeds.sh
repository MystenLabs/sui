#!/usr/bin/env bash
set -euo pipefail

source "${TEST_SANDBOX_DIR}/common.sh"

print_section "advance-checkpoint"

stdout=""
stderr=""
if ! run_cmd stdout stderr "$SUI_FORKING_BIN" advance-checkpoint --server-url "$FORKING_SERVER_URL"; then
  echo "$stderr" >&2
  echo "advance-checkpoint command failed" >&2
  exit 1
fi

echo "advance_checkpoint_succeeded=true"
