#!/usr/bin/env bash
set -euo pipefail

# FORKING_TEST_MODE=start_only
source "${TEST_SANDBOX_DIR}/common.sh"

print_section "custom-network-requires-fullnode"

stdout=""
stderr=""
if run_cmd stdout stderr "$SUI_FORKING_BIN" start --network "http://example.com/graphql"; then
  echo "start command unexpectedly succeeded without --fullnode-url for custom network" >&2
  exit 1
fi

require_contains "$stderr" "fullnode_url is required when network is custom"
echo "custom_network_requires_fullnode_url=true"
