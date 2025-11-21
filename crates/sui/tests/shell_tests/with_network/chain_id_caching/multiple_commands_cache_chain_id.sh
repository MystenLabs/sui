#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test that various client commands all properly cache chain_id

echo "=== Initial state ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
else
    echo "chain_id field exists: NO"
fi

echo ""
echo "=== Testing objects command ==="
sui client --client.config $CONFIG objects > output.txt 2>&1
echo "Command executed"

echo ""
echo "=== Config state after objects ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi

echo ""
echo "=== Testing gas command ==="
sui client --client.config $CONFIG gas > output.txt 2>&1
echo "Command executed"

echo ""
echo "=== Config state after gas ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi

echo ""
echo "=== Testing balance command ==="
sui client --client.config $CONFIG balance > output.txt 2>&1
echo "Command executed"

echo ""
echo "=== Final config state ==="
if grep -q "chain_id:" $CONFIG 2>&1; then
    echo "chain_id field exists: YES"
    grep "chain_id:" $CONFIG | sed 's/chain_id: .*/chain_id: <REDACTED>/'
else
    echo "chain_id field exists: NO"
fi
