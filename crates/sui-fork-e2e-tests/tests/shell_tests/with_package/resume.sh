#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Publish on a fork, stop it, and restart it from the same data directory to check that the fork
# resumes with its state, and that restarting with other seeds or another checkpoint is refused.
set -euo pipefail
source ./lib.sh

sender=$(on_localnet active-address)
fork_start --address "$sender"
fork_env
fork_point=$FORK_CHECKPOINT
add_env_to_toml counter fork on_fork
gas=$(gas_coin on_fork)
run_json publish.json on_fork publish counter --gas "$gas" --gas-budget 100000000
package=$(published_package_id publish.json)
fork_cmd advance-checkpoint > /dev/null
tip=$(fork_status_field checkpoint_sequence_number)
fork_stop

echo "=== restart with the same --data-dir and no seed flags ==="
fork_start_plain --checkpoint "$fork_point"
assert_grep "Resuming forked network from " "$FORK_LOG" "start reports that it resumed"
fork_env
fork_cmd status > status.json
assert_eq "$(jq -r .forked_at_checkpoint status.json)" "$fork_point" "the fork point is preserved"
assert_eq "$(jq -r .checkpoint_sequence_number status.json)" "$tip" \
  "the tip is preserved across the restart"
assert_gt "$tip" "$fork_point" "the resumed fork is past its fork point"
assert_eq "$(object_field on_fork "$package" .objType)" package \
  "the package published before the restart is still readable"
assert_eq "$(gas_count on_fork)" "$(gas_count on_localnet)" \
  "the seeded coins are still listed after the restart"
gas=$(gas_coin on_fork)
run_json create.json on_fork call --package "$package" --module counter --function create \
  --gas "$gas" --gas-budget 50000000
assert_eq "$(tx_status_of create.json)" success "transactions execute after the restart"
assert_eq "$(($(fork_status_field checkpoint_sequence_number) - tip))" 1 \
  "the transaction sealed the next checkpoint"
fork_stop

echo "=== restart with seed flags is refused ==="
if sui-fork start --network "$GRAPHQL_URL" --rpc-addr "$FORK_RPC_ADDR" --data-dir "$FORK_DATA_DIR" \
  --checkpoint "$fork_point" --address "$sender" > reseed.log 2>&1; then
  fail "start with seed flags on an existing data dir must fail"
  cat reseed.log
else
  grep -o "A seed manifest already exists at .*" reseed.log
fi

echo "=== restart at another checkpoint is refused ==="
if sui-fork start --network "$GRAPHQL_URL" --rpc-addr "$FORK_RPC_ADDR" --data-dir "$FORK_DATA_DIR" \
  --checkpoint $((fork_point + 1)) > other_checkpoint.log 2>&1; then
  fail "start at another checkpoint on an existing data dir must fail"
  cat other_checkpoint.log
else
  grep -o "Seed manifest checkpoint .*" other_checkpoint.log | scrub
fi

exit "$FAILED"
