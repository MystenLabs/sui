#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test that --json flag outputs a proper json (without emitting any logs in stdout) 

sui client --client.config $CONFIG \
  test-publish --build-env testnet --pubfile-path Pub.local.toml a --json \
  > output.json

# Make sure the output is a valid json
jq -e . output.json >/dev/null || cat output.json >&2;
