#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test an ephemeral upgrade workflow. We have
# B --> A
# C --> B
# C --> A

echo "=== expect to fail when upgrading a because it is not even published yet ==="
sui client --client.config $CONFIG \
  test-upgrade --build-env testnet --pubfile-path Pub.local.toml a

echo "=== expect to fail because we should never upgrade with --test flag ==="
sui client --client.config $CONFIG \
  test-upgrade --build-env testnet --test --pubfile-path Pub.local.toml a
