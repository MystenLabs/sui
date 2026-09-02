#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Compare what an unseeded fork, a fork seeded with one coin (--object), and a fork seeded with a
# whole address (--address) list for the same account, and what each writes to the seed manifest.
set -euo pipefail
source ./lib.sh

address=$(on_localnet active-address)
localnet_coins=$(gas_count on_localnet)
first_coin=$(gas_coin on_localnet)

echo "=== no seeds ==="
FORK_DIR="$FORK_DATA_DIR/none" fork_start
fork_env
fork_point=$FORK_CHECKPOINT
assert_eq "$(gas_count on_fork)" 0 "an unseeded address lists no gas coins on the fork"
assert_eq "$(on_fork objects --json | jq 'length')" 0 \
  "an unseeded address lists no objects on the fork"
assert_eq "$(object_field on_fork "$first_coin" .objectId)" "$first_coin" \
  "a direct read still resolves an unseeded coin at the fork point"
manifest="$FORK_DATA_DIR/none/seed_manifest.json"
assert_eq "$(jq '.entries | length' "$manifest")" 0 "the manifest is written with no entries"
assert_eq "$(jq -r .checkpoint "$manifest")" "$fork_point" "the manifest records the fork point"
fork_stop

echo "=== --object with one coin ==="
FORK_DIR="$FORK_DATA_DIR/object" fork_start --checkpoint "$fork_point" --object "$first_coin"
fork_env
assert_eq "$(gas_count on_fork)" 1 "only the seeded coin is listed"
assert_eq "$(gas_coin on_fork)" "$first_coin" "the listed coin is the seeded one"
manifest="$FORK_DATA_DIR/object/seed_manifest.json"
assert_eq "$(jq '.entries | length' "$manifest")" 1 "the manifest has one entry"
assert_eq "$(jq -r '.entries[0].object_ref[0]' "$manifest")" "$first_coin" \
  "the manifest entry is the seeded coin"
fork_stop

echo "=== --address ==="
FORK_DIR="$FORK_DATA_DIR/address" fork_start --checkpoint "$fork_point" --address "$address"
fork_env
assert_eq "$(gas_count on_fork)" "$localnet_coins" "every localnet gas coin is listed on the fork"
assert_eq "$(gas_total on_fork)" "$(gas_total on_localnet)" "the SUI totals match"
manifest="$FORK_DATA_DIR/address/seed_manifest.json"
assert_eq "$(jq -r '.addresses | join(",")' "$manifest")" "$address" \
  "the manifest records the seeded address"
if [ "$(jq '.entries | length' "$manifest")" -ge "$localnet_coins" ]; then
  ok "the manifest has at least one entry per gas coin"
else
  fail "the manifest is missing coin entries"
fi

exit "$FAILED"
