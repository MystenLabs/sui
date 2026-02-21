#!/usr/bin/env bash
set -euo pipefail

# FORKING_TEST_MODE=start_only
source "${TEST_SANDBOX_DIR}/common.sh"

print_section "invalid-network"

stdout=""
stderr=""
if run_cmd stdout stderr "$SUI_FORKING_BIN" start --network not-a-network; then
  echo "start command unexpectedly succeeded for invalid network" >&2
  exit 1
fi

require_contains "$stderr" "invalid network"
echo "invalid_network_rejected=true"
