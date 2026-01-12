#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test an ephemeral upgrade workflow. We have
# B --> A
# C --> B
# C --> A

chain_id=$(sui client --client.config $CONFIG chain-identifier)

add_env_to_toml() {
  echo "[environments]" >> $1/Move.toml
  echo "localnet = \"$chain_id\"" >> $1/Move.toml
}

add_env_to_toml a

echo "=== expect to fail when upgrading a because it is not even published yet ==="
sui client --client.config $CONFIG upgrade a

echo "=== expect to fail because we should never upgrade with --test flag ==="
sui client --client.config $CONFIG upgrade a --test
