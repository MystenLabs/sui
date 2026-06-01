#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test publication failures
# B --> A
# C --> B
# C --> A
#
# D --> B
# D --> A
#
# E --> C
# E --> D
#
# We publish A, B, C, D, E in order

GAS=$(sui client --client.config $CONFIG faucet --coin-id)
chain_id=$(sui client --client.config $CONFIG chain-identifier)

add_env_to_toml() {
  echo "[environments]" >> $1/Move.toml
  echo "localnet = \"$chain_id\"" >> $1/Move.toml
}

echo "=== Should fail to publish because dependencies are not published. ==="

add_env_to_toml a
add_env_to_toml b

sui client --client.config $CONFIG publish --gas $GAS b

# Publish A, so we can try to publish twice
sui client --client.config $CONFIG publish --gas $GAS a > output.log 2>&1 || cat output.log

# Try to publish A again.
sui client --client.config $CONFIG publish --gas $GAS a
