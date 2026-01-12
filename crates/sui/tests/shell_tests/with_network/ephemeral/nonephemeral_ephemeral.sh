#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Publishing a package and then doing a test-publish succeed

chain_id=$(sui client --client.config $CONFIG chain-identifier)

echo "[environments]" >> a/Move.toml
echo "localnet = \"$chain_id\"" >> a/Move.toml

echo "=== real publish ==="
sui client --client.config $CONFIG publish a \
  > /dev/null || echo "failed real publish"

echo "=== ephemeral publish ==="
sui client --client.config $CONFIG \
  test-publish --build-env localnet --pubfile-path Pub.local.toml a \
  > /dev/null || echo "failed test publish"
