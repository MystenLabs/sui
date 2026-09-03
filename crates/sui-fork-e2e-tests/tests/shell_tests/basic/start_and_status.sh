#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Fork the localnet at its latest checkpoint and check what `start --json` and `status` report.
set -euo pipefail
source ./lib.sh
localnet_setup

fork_start
fork_cmd status > status.json

assert_eq "$(fork_start_json | jq -r 'keys | join(",")')" "checkpoint,network,rpc_addr" \
  "start --json reports the checkpoint, network, and rpc address"
assert_eq "$(fork_start_json | jq -r .network)" "$GRAPHQL_URL" \
  "start reports the source network"
assert_eq "$(jq -r 'keys | join(",")' status.json)" \
  "checkpoint_sequence_number,epoch,forked_at_checkpoint,timestamp,timestamp_ms" \
  "status --json reports the epoch, checkpoint, clock, and fork point"
assert_eq "$(jq -r .forked_at_checkpoint status.json)" "$FORK_CHECKPOINT" \
  "status reports the fork point that start reported"
assert_eq "$(jq -r .checkpoint_sequence_number status.json)" "$FORK_CHECKPOINT" \
  "nothing is sealed yet, so the tip is the fork point"
assert_gt "$(jq -r .timestamp_ms status.json)" 0 "status reports a positive clock"

echo "=== status ==="
sui-fork status --rpc-addr "$FORK_RPC" | scrub

exit "$FAILED"
