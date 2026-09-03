#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Seed a fork with an address whose key the client does not hold, then act as that address with
# `--sender` and `--skip-signing`, and check that the same transaction needs the key without it.
set -euo pipefail
source ./lib.sh
localnet_setup

sender=$(on_localnet active-address)

echo "=== fund an address whose key lives in another keystore ==="
mkdir victim
sui client --client.config victim/client.yaml -y envs > /dev/null
victim=$(sui client --client.config victim/client.yaml active-address)
on_localnet faucet --address "$victim" --url "$FAUCET_URL" > /dev/null
victim_coin=$(gas_coin on_localnet "$victim")
wait_for_graphql_tx "$(object_field on_localnet "$victim_coin" .prevTx)" > /dev/null
victim_localnet=$(gas_total on_localnet "$victim")
assert_gt "$victim_localnet" 0 "the victim holds SUI on the localnet"
if on_localnet addresses --json | jq -e --arg v "$victim" '.addresses[] | select(.[1] == $v)' \
  > /dev/null; then
  fail "the victim's key must not be in the client keystore"
else
  ok "the victim's key is not in the client keystore"
fi

echo "=== fork seeded with the victim ==="
fork_start --address "$victim"
fork_env
assert_eq "$(gas_total on_fork "$victim")" "$victim_localnet" \
  "the victim holds the same SUI on the fork"
amount=1000000
if on_fork transfer-sui --sender "$victim" --to "$sender" --sui-coin-object-id "$victim_coin" \
  --amount "$amount" --gas-budget 10000000 > signed.log 2>&1; then
  fail "signing as the victim must fail without its key"
  cat signed.log
else
  assert_grep "No keystore found for the provided key identity" signed.log \
    "signing as the victim needs its key"
fi
run_json impersonated.json on_fork transfer-sui --sender "$victim" --to "$sender" \
  --sui-coin-object-id "$victim_coin" --amount "$amount" --gas-budget 10000000 --skip-signing
assert_eq "$(tx_status_of impersonated.json)" success \
  "the fork executes the transfer as the victim without its key"
assert_eq "$(jq -r .transaction.data.sender impersonated.json)" "$victim" \
  "the transaction records the victim as the sender"
assert_gt "$((victim_localnet - $(gas_total on_fork "$victim")))" "$amount" \
  "the victim paid the amount plus gas on the fork"
assert_eq "$(gas_total on_localnet "$victim")" "$victim_localnet" \
  "the victim is untouched on the localnet"

exit "$FAILED"
