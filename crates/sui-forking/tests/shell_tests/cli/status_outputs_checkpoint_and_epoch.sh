#!/usr/bin/env bash
set -euo pipefail

source "${TEST_SANDBOX_DIR}/common.sh"

print_section "status"

stdout=""
stderr=""
if ! run_cmd stdout stderr "$SUI_FORKING_BIN" status --server-url "$FORKING_SERVER_URL"; then
  echo "$stderr" >&2
  echo "status command failed" >&2
  exit 1
fi
require_contains "$stdout" "Checkpoint:"
require_contains "$stdout" "Epoch:"

echo "status_has_checkpoint=true"
echo "status_has_epoch=true"
