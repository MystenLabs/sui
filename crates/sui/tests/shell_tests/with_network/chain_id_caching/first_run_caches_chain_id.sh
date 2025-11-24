#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test that running a client command for the first time caches the chain_id to client.yaml

echo "=== Initial config state ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
else
    echo "chain_id field exists: NO"
fi

echo ""
echo "=== Running balance command ==="
sui client --client.config $CONFIG balance > output.txt 2>&1
echo "Command executed"
rm -f output.txt

echo ""
echo "=== Final config state ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi
