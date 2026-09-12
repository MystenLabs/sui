#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Send an unsigned transaction to a fork with `--skip-signing`, and check that the localnet
# refuses the same transaction and that `ptb` does not offer the flag.
set -euo pipefail
source ./lib.sh
localnet_setup

sender=$(on_localnet active-address)
recipient=$(other_address "$sender")
fork_start --address "$sender"
fork_env
coin=$(gas_coin on_fork)
amount=1000000

echo "=== the fork executes an unsigned transaction as its sender ==="
run_json unsigned.json on_fork transfer-sui --to "$recipient" --sui-coin-object-id "$coin" \
  --amount "$amount" --gas-budget 10000000 --skip-signing
assert_eq "$(tx_status_of unsigned.json)" success "the unsigned transfer executed on the fork"
digest=$(tx_digest_of unsigned.json)
assert_eq "$(jq -r .transaction.data.sender unsigned.json)" "$sender" \
  "the transaction records the declared sender"
assert_eq "$(jq -r '.transaction.txSignatures | length' unsigned.json)" 0 \
  "the transaction carries no signatures"
on_fork tx-block "$digest" --json > fork_tx.json
assert_eq "$(jq -r .digest fork_tx.json)" "$digest" "tx-block resolves the unsigned transaction"
assert_eq "$(gas_total on_fork "$recipient")" "$amount" "the recipient holds the amount on the fork"

echo "=== the localnet refuses an unsigned transaction ==="
localnet_coin=$(gas_coin on_localnet)
if on_localnet transfer-sui --to "$recipient" --sui-coin-object-id "$localnet_coin" \
  --amount "$amount" --gas-budget 10000000 --skip-signing > localnet_unsigned.log 2>&1; then
  fail "the localnet must reject a transaction without signatures"
  cat localnet_unsigned.log
else
  ok "the localnet rejects the unsigned transaction"
  # The message repeats inside the one error line, so the matches are deduplicated.
  grep -o -m1 'Invalid user signature: Expect [0-9]* signer signatures but got [0-9]*' \
    localnet_unsigned.log | sort -u | scrub || cat localnet_unsigned.log
fi
assert_eq "$(gas_count on_localnet "$recipient")" 0 "the localnet recipient received nothing"

echo "=== ptb does not offer --skip-signing ==="
if on_fork ptb --transfer-objects "[$coin]" "@$recipient" --gas-budget 10000000 --skip-signing \
  > ptb.log 2>&1; then
  fail "ptb must not accept --skip-signing"
  cat ptb.log
else
  assert_grep "Unknown command '--skip-signing'" ptb.log "ptb rejects --skip-signing"
fi

exit "$FAILED"
