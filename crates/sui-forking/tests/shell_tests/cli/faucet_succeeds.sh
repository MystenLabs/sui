#!/usr/bin/env bash
set -euo pipefail

source "${TEST_SANDBOX_DIR}/common.sh"

print_section "faucet"

recipient="0x0000000000000000000000000000000000000000000000000000000000000041"
stdout=""
stderr=""
if ! run_cmd stdout stderr "$SUI_FORKING_BIN" faucet --server-url "$FORKING_SERVER_URL" --address "$recipient" --amount 1000; then
  echo "$stderr" >&2
  echo "faucet command failed" >&2
  exit 1
fi

echo "faucet_succeeded=true"
