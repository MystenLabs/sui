#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Seal checkpoints and move the clock on a fork with `advance-checkpoint` and `advance-clock`.
set -euo pipefail
source ./lib.sh

fork_start
before_checkpoint=$(fork_status_field checkpoint_sequence_number)
before_clock=$(fork_status_field timestamp_ms)

fork_cmd advance-checkpoint > advance_checkpoint.json
sealed=$(jq -r .checkpoint_sequence_number advance_checkpoint.json)
echo "advance-checkpoint moves the tip by $((sealed - before_checkpoint))"
assert_eq "$(jq -r .timestamp_ms advance_checkpoint.json)" "$before_clock" \
  "advance-checkpoint leaves the clock unchanged"

fork_cmd advance-clock --duration-ms 5000 > advance_clock.json
after_clock=$(jq -r .timestamp_ms advance_clock.json)
echo "advance-clock --duration-ms 5000 moves the clock by $((after_clock - before_clock)) ms"
assert_eq "$(jq -r '.tx_digest | test("^[1-9A-HJ-NP-Za-km-z]{43,44}$")' advance_clock.json)" true \
  "advance-clock reports the digest of the clock transaction"

fork_cmd status > status.json
tip=$(jq -r .checkpoint_sequence_number status.json)
clock=$(jq -r .timestamp_ms status.json)
echo "status shows the tip moved by $((tip - before_checkpoint))"
echo "status shows the clock moved by $((clock - before_clock)) ms"
assert_eq "$(jq -r .forked_at_checkpoint status.json)" "$FORK_CHECKPOINT" \
  "the fork point is unchanged"

fork_cmd advance-clock > advance_default.json
default_clock=$(jq -r .timestamp_ms advance_default.json)
echo "advance-clock without a duration moves the clock by $((default_clock - after_clock)) ms"

echo "=== advance-checkpoint ==="
sui-fork advance-checkpoint --rpc-addr "$FORK_RPC" | scrub
echo "=== advance-clock ==="
sui-fork advance-clock --duration-ms 10 --rpc-addr "$FORK_RPC" | scrub

exit "$FAILED"
