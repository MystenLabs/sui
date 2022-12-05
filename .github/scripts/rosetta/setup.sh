#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "run Sui genesis"
cargo run --bin sui genesis

echo "generate rosetta configuration"
cargo run --bin sui-rosetta -- generate-rosetta-cli-config --online-url http://127.0.0.1:9002 --offline-url http://127.0.0.1:9003

echo "install rosetta-cli"
curl -sSfL https://raw.githubusercontent.com/coinbase/rosetta-cli/master/scripts/install.sh | sh -s