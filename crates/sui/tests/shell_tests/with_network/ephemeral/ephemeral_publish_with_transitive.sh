#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test an ephemeral publication workflow. We have
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
# We publish A, B, C, D, E (should transitively publish all).

echo "publishing e (using --publish-unpublished-deps)"
sui client --client.config $CONFIG test-publish --build-env testnet \
  --pubfile-path Pub.local.toml --publish-unpublished-deps e \
  > /dev/null || echo "failed to publish e (or one of its transitive dependencies)"

echo "attempting a second ephemeral publish of e, which should fail"
sui client --client.config $CONFIG test-publish --build-env testnet \
  --pubfile-path Pub.local.toml --publish-unpublished-deps e
