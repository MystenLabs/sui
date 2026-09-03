#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Publish a package on the localnet, fork with the publisher's objects, and upgrade the package on
# the fork with the UpgradeCap that the localnet publish created. The fork reports the localnet's
# chain identifier, so the CLI resolves the `fork` env to the package's `localnet` environment and
# finds its publication record without any change to Published.toml.
set -euo pipefail
source ./lib.sh
localnet_setup

sender=$(on_localnet active-address)

echo "=== localnet: publish before forking ==="
add_env_to_toml counter localnet on_localnet
run_json publish.json on_localnet publish counter --gas-budget 100000000
package_v1=$(published_package_id publish.json)
cap=$(created_object_id publish.json "::package::UpgradeCap")
fork_point=$(wait_for_graphql_tx "$(tx_digest_of publish.json)")
echo "=== counter/Published.toml after the localnet publish ==="
extract_published counter/Published.toml

echo "=== fork: upgrade the localnet package ==="
fork_start --checkpoint "$fork_point" --address "$sender"
fork_env
assert_eq "$(object_field on_fork "$cap" .owner.AddressOwner)" "$sender" \
  "the UpgradeCap from the localnet publish is seeded on the fork"
echo 'module counter::v2; public fun two(): u64 { 2 }' > counter/sources/v2.move
gas=$(gas_coin on_fork)
run_json upgrade.json on_fork upgrade counter --gas "$gas" --gas-budget 100000000
assert_eq "$(tx_status_of upgrade.json)" success "the localnet package upgraded on the fork"
assert_grep 'building for `localnet` instead' upgrade.json.err \
  "the CLI resolved the fork env to the localnet environment by chain id"
package_v2=$(published_package_id upgrade.json)
assert_ne "$package_v2" "$package_v1" "the upgrade produced a new package id"
assert_eq "$(mutated_object_id upgrade.json "::package::UpgradeCap")" "$cap" \
  "the upgrade used the UpgradeCap from the localnet publish"
assert_eq "$(object_field on_fork "$package_v2" '.version | tostring')" 2 \
  "the upgraded package is at version 2 on the fork"
run_json call.json on_fork call --package "$package_v2" --module v2 --function two \
  --gas "$gas" --gas-budget 50000000
assert_eq "$(tx_status_of call.json)" success "the new module is callable on the fork"
echo "=== counter/Published.toml after the fork upgrade ==="
extract_published counter/Published.toml

echo "=== the localnet is untouched ==="
assert_eq "$(object_field on_localnet "$package_v1" .objType)" package \
  "the original package is still readable on the localnet"
if on_localnet object "$package_v2" > localnet_v2.log 2>&1; then
  fail "the upgraded package must not exist on the localnet"
  cat localnet_v2.log
else
  assert_grep "not found" localnet_v2.log "the upgraded package does not exist on the localnet"
fi
assert_eq "$(object_field on_localnet "$cap" '.version | tostring')" \
  "$(object_field on_fork "$cap" '.version | tostring' | awk '{print $1 - 1}')" \
  "the localnet's UpgradeCap is one version behind the fork's"

exit "$FAILED"
