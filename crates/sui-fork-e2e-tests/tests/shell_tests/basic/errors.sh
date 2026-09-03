#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Check the errors a fork reports for a missing checkpoint and for a client command against a
# stopped fork.
set -euo pipefail
source ./lib.sh
localnet_setup

echo "=== start at a checkpoint the localnet does not have ==="
if sui-fork start --network "$GRAPHQL_URL" --rpc-addr "$FORK_RPC_ADDR" \
  --data-dir "$FORK_DATA_DIR/future" --checkpoint 4294967295 > future.log 2>&1; then
  fail "start must fail for a checkpoint the localnet does not have"
  cat future.log
else
  assert_grep "checkpoint 4294967295 not found" future.log "start reports the missing checkpoint"
fi

echo "=== status against a stopped fork ==="
fork_start
fork_stop
if sui-fork status --rpc-addr "$FORK_RPC" > stopped.log 2>&1; then
  fail "status must fail when nothing listens on the fork's port"
  cat stopped.log
else
  assert_grep "Connection refused" stopped.log "status fails when nothing listens on the port"
fi

exit "$FAILED"
