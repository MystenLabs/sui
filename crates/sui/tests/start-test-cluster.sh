#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

DIR=$(mktemp -d)
SUI="./target/debug/sui"
$SUI genesis --working-dir "$DIR" --epoch-duration-ms 3600000 --committee-size 1
$SUI start --network.config "$DIR" --with-faucet &
PID=$!

# Wait for the faucet to be ready
for i in $(seq 1 60); do
    curl -sf http://127.0.0.1:9123 > /dev/null 2>&1 && break
    sleep 1
done

echo "SUI_TEST_CLUSTER_CONFIG_DIR=$DIR" >> "$NEXTEST_ENV"

# Keep the script process alive until nextest sends SIGTERM after tests finish.
# This ensures the `sui start` child process is cleaned up automatically.
wait $PID
