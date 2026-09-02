#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Check the errors a fork reports for a missing checkpoint, for transactions that need gas
# estimation, and for a client command against a stopped fork.
set -euo pipefail
source ./lib.sh

echo "=== start at a checkpoint the localnet does not have ==="
if sui-fork start --network "$GRAPHQL_URL" --rpc-addr "$FORK_RPC_ADDR" \
  --data-dir "$FORK_DATA_DIR/future" --checkpoint 4294967295 > future.log 2>&1; then
  fail "start must fail for a checkpoint the localnet does not have"
  cat future.log
else
  assert_grep "checkpoint 4294967295 not found" future.log "start reports the missing checkpoint"
fi

echo "=== transactions need an explicit gas budget and gas coin ==="
sender=$(on_localnet active-address)
recipient=$(other_address "$sender")
fork_start --address "$sender"
fork_env
coin=$(gas_coin on_fork)
if on_fork transfer-sui --to "$recipient" --sui-coin-object-id "$coin" --amount 1000 \
  > no_budget.log 2>&1; then
  fail "transfer-sui without --gas-budget must fail"
  cat no_budget.log
else
  assert_grep "Could not determine the gas budget" no_budget.log \
    "gas estimation fails because the fork does not simulate transactions"
fi
if on_fork transfer --to "$recipient" --object-id "$coin" --gas-budget 10000000 \
  > no_gas.log 2>&1; then
  fail "transfer without --gas must fail"
  cat no_gas.log
else
  assert_grep "Gas selection failed" no_gas.log \
    "gas selection fails because the fork does not simulate transactions"
fi
if on_fork transfer-sui --to "$recipient" --sui-coin-object-id "$coin" --amount 1000 \
  --gas-budget 10000000 --dry-run > dry_run.log 2>&1; then
  fail "--dry-run must fail"
  cat dry_run.log
else
  assert_grep "simulate_transaction is not supported" dry_run.log "--dry-run is rejected"
fi

echo "=== status against a stopped fork ==="
fork_stop
if sui-fork status --rpc-addr "$FORK_RPC" > stopped.log 2>&1; then
  fail "status must fail when nothing listens on the fork's port"
  cat stopped.log
else
  assert_grep "Connection refused" stopped.log "status fails when nothing listens on the port"
fi

exit "$FAILED"
