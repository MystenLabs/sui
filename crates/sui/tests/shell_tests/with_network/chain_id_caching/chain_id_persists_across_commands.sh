#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test that chain_id persists across multiple commands

echo "=== First command (chain-identifier) ==="
sui client --client.config $CONFIG chain-identifier > chain_id_1.txt 2>&1
echo "Command executed"

echo ""
echo "=== Config state after first command ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi

echo ""
echo "=== Second command (chain-identifier again) ==="
sui client --client.config $CONFIG chain-identifier > chain_id_2.txt 2>&1
echo "Command executed"

echo ""
echo "=== Verification ==="
if diff chain_id_1.txt chain_id_2.txt > diff_output.txt 2>&1; then
    echo "Chain IDs match: PASS"
else
    echo "Chain IDs differ: FAIL"
fi

echo ""
echo "=== Third command (objects) ==="
sui client --client.config $CONFIG objects > objects_output.txt 2>&1
echo "Command executed"

echo ""
echo "=== Final config state ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi
