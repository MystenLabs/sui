#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Send SUI on a fork seeded with the sender's coins, and check that balances move on the fork
# while the localnet is untouched.
set -euo pipefail
source ./lib.sh
localnet_setup

sender=$(on_localnet active-address)
recipient=$(other_address "$sender")
fork_start --address "$sender"
fork_env

localnet_before=$(gas_total on_localnet)
fork_before=$(gas_total on_fork)
recipient_localnet_before=$(gas_count on_localnet "$recipient")
assert_eq "$fork_before" "$localnet_before" \
  "the seeded sender holds the same SUI on the fork as on the localnet"
assert_eq "$(gas_count on_fork)" "$(gas_count on_localnet)" \
  "the seeded sender lists the same coins on the fork"
assert_eq "$(gas_count on_fork "$recipient")" 0 \
  "the unseeded recipient lists no coins on the fork"

coin=$(gas_coin on_fork)
amount=1000000
tip_before=$(fork_status_field checkpoint_sequence_number)
run_json transfer.json on_fork transfer-sui --to "$recipient" --sui-coin-object-id "$coin" \
  --amount "$amount" --gas-budget 10000000
digest=$(tx_digest_of transfer.json)
assert_eq "$(tx_status_of transfer.json)" success "transfer-sui executed on the fork"
tip_after=$(fork_status_field checkpoint_sequence_number)
assert_eq "$((tip_after - tip_before))" 1 "the transfer sealed exactly one checkpoint"

fork_after=$(gas_total on_fork)
assert_gt "$((fork_before - fork_after))" "$amount" \
  "the sender paid the amount plus gas on the fork"
assert_eq "$(gas_count on_fork "$recipient")" 1 "the recipient owns one coin on the fork"
assert_eq "$(gas_total on_fork "$recipient")" "$amount" \
  "the recipient's coin holds the transferred amount"
assert_eq "$(gas_total on_localnet)" "$localnet_before" "the localnet sender is unchanged"
assert_eq "$(gas_count on_localnet "$recipient")" "$recipient_localnet_before" \
  "the localnet recipient is unchanged"

on_fork tx-block "$digest" --json > fork_tx.json
assert_eq "$(jq -r .digest fork_tx.json)" "$digest" "tx-block resolves the transfer on the fork"
if on_localnet tx-block "$digest" > localnet_tx.log 2>&1; then
  fail "the localnet must not know the fork transaction"
  cat localnet_tx.log
else
  assert_grep "not found" localnet_tx.log "the localnet does not know the fork transaction"
fi

exit "$FAILED"
