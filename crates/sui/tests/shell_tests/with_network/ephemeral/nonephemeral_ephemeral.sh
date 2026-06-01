#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Publishing a package and then doing a test-publish succeed

GAS=$(sui client --client.config $CONFIG faucet --coin-id)
chain_id=$(sui client --client.config $CONFIG chain-identifier)

echo "[environments]" >> a/Move.toml
echo "localnet = \"$chain_id\"" >> a/Move.toml

echo "=== real publish ==="
sui client --client.config $CONFIG publish --gas $GAS a \
  > out.log 2>&1 || cat out.log

echo "=== ephemeral publish ==="
sui client --client.config $CONFIG \
  test-publish --gas $GAS --build-env localnet --pubfile-path Pub.local.toml a \
  > out.log 2>&1 || cat out.log
