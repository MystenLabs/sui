#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Fork the localnet without seeds and seed it while it runs with `sui-fork seed`: one coin, the
# whole address, and a shared object's dynamic fields. Check that each seed shows up, that seeding
# again changes nothing, so no balance is counted twice, and that state the fork changed itself is
# left alone.
set -euo pipefail
source ./lib.sh
localnet_setup

# trailing_u64 <where> <object id>: the last eight bytes of an object's BCS as a number, which is
# the `value` of a Field<u64, u64>.
trailing_u64() {
  "$1" object "$2" --bcs --json | jq -r \
    '.data.Move.contents[-8:] as $b | reduce range(0; 8) as $i (0; . + $b[$i] * pow(2; 8 * $i))'
}
# field_count <where> <parent id>: how many dynamic fields `dynamic-field` lists for the parent.
field_count() { "$1" dynamic-field "$2" --json | jq -r '.dynamicFields | length'; }
# field_id_for <where> <parent id> <key>: the field whose u64 name is `key` (below 256), read
# from the first byte of the base64 BCS name.
field_id_for() {
  "$1" dynamic-field "$2" --json | jq -r --arg key "$3" \
    '.dynamicFields[] | select((.name.value | @base64d | explode | .[0]) == ($key | tonumber)) | .fieldId'
}
# seed [flags]: seed the running fork, keeping the JSON outcome in seed.json for the counts below.
seed() { fork_cmd seed "$@" > seed.json; }
# seeded_count, present_count: objects the last seed loaded, and objects it found already present.
seeded_count() { jq '[.requests[].seeded | length] | add' seed.json; }
present_count() { jq '[.requests[].already_present | length] | add' seed.json; }

address=$(on_localnet active-address)

echo "=== localnet: publish, create, and fill the registry before forking ==="
add_env_to_toml registry localnet on_localnet
run_json publish.json on_localnet publish registry --gas-budget 100000000
package=$(published_package_id publish.json)
run_json create.json on_localnet call --package "$package" --module registry --function create \
  --gas-budget 50000000
registry=$(created_object_id create.json "::registry::Registry")
run_json add1.json on_localnet call --package "$package" --module registry --function add \
  --args "$registry" 1 10 --gas-budget 50000000
run_json add2.json on_localnet call --package "$package" --module registry --function add \
  --args "$registry" 2 20 --gas-budget 50000000
fork_point=$(wait_for_graphql_tx "$(tx_digest_of add2.json)")
localnet_coins=$(gas_count on_localnet)
first_coin=$(gas_coin on_localnet)
field1=$(field_id_for on_localnet "$registry" 1)

echo "=== fork without seeds ==="
fork_start --checkpoint "$fork_point"
fork_env
assert_eq "$(gas_count on_fork)" 0 "an unseeded fork lists no gas coins"
assert_eq "$(field_count on_fork "$registry")" 0 "an unseeded fork lists no registry fields"

echo "=== seed one coin ==="
seed --object "$first_coin"
assert_eq "$(seeded_count)" 1 "the coin is seeded"
assert_eq "$(gas_count on_fork)" 1 "the fork lists the seeded coin"
assert_eq "$(gas_coin on_fork)" "$first_coin" "the listed coin is the seeded one"

echo "=== seed the whole address ==="
seed --address "$address"
assert_eq "$(present_count)" 1 "the coin seeded before is already present"
assert_gt "$(seeded_count)" 0 "the rest of the address is seeded"
assert_eq "$(gas_count on_fork)" "$localnet_coins" "every localnet gas coin is listed on the fork"
assert_eq "$(gas_total on_fork)" "$(gas_total on_localnet)" "the SUI totals match"
seed --address "$address"
assert_eq "$(seeded_count)" 0 "seeding the address again seeds nothing"
assert_eq "$(gas_total on_fork)" "$(gas_total on_localnet)" "seeding again counts no balance twice"

echo "=== seed the registry's dynamic fields ==="
seed --dynamic-fields-of "$registry"
assert_eq "$(seeded_count)" 2 "both pre-fork fields are seeded"
assert_eq "$(field_count on_fork "$registry")" 2 "the fork lists both pre-fork fields"
assert_eq "$(field_id_for on_fork "$registry" 1)" "$field1" "the fork lists field 1 under its id"

echo "=== state the fork changed itself is left alone ==="
gas=$(gas_coin on_fork)
run_json bump.json on_fork call --package "$package" --module registry --function bump \
  --args "$registry" 1 --gas "$gas" --gas-budget 50000000
assert_eq "$(tx_status_of bump.json)" success "bump executed against the seeded field"
assert_eq "$(trailing_u64 on_fork "$field1")" 11 "field 1 reads 11 on the fork after the bump"
seed --dynamic-fields-of "$registry"
assert_eq "$(seeded_count)" 0 "seeding the fields again seeds nothing"
assert_eq "$(present_count)" 2 "both fields, the bumped one included, are already present"
assert_eq "$(trailing_u64 on_fork "$field1")" 11 "the seed did not roll field 1 back to 10"

exit "$FAILED"
