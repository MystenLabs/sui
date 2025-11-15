#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script to batch transfer SUI to multiple addresses
# Usage: ./batch-transfer.sh <addresses-file> <amount> [network]

set -e

ADDRESSES_FILE=$1
AMOUNT=$2
NETWORK=${3:-devnet}

if [ -z "$ADDRESSES_FILE" ] || [ -z "$AMOUNT" ]; then
    echo "Usage: $0 <addresses-file> <amount> [network]"
    echo "  addresses-file: File containing one address per line"
    echo "  amount: Amount of SUI to send to each address"
    echo "  network: devnet, testnet, or mainnet (default: devnet)"
    exit 1
fi

if [ ! -f "$ADDRESSES_FILE" ]; then
    echo "Error: File $ADDRESSES_FILE not found"
    exit 1
fi

# Check if sui CLI is available
if ! command -v sui &> /dev/null; then
    echo "Error: sui CLI not found. Please install it first."
    exit 1
fi

echo "Batch transfer configuration:"
echo "  Addresses file: $ADDRESSES_FILE"
echo "  Amount per address: $AMOUNT SUI"
echo "  Network: $NETWORK"
echo ""

# Count addresses
TOTAL=$(wc -l < "$ADDRESSES_FILE")
echo "Total addresses to process: $TOTAL"
echo ""

# Ask for confirmation
read -p "Do you want to proceed? (y/n) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Transfer cancelled"
    exit 0
fi

# Process each address
COUNT=0
SUCCESS=0
FAILED=0

while IFS= read -r ADDRESS; do
    # Skip empty lines
    if [ -z "$ADDRESS" ]; then
        continue
    fi

    COUNT=$((COUNT + 1))
    echo "[$COUNT/$TOTAL] Transferring $AMOUNT SUI to $ADDRESS..."

    if sui client transfer-sui --to "$ADDRESS" --amount "$AMOUNT" --network "$NETWORK"; then
        SUCCESS=$((SUCCESS + 1))
        echo "  ✓ Success"
    else
        FAILED=$((FAILED + 1))
        echo "  ✗ Failed"
    fi

    # Add a small delay to avoid rate limiting
    sleep 1
done < "$ADDRESSES_FILE"

echo ""
echo "Batch transfer complete:"
echo "  Total: $TOTAL"
echo "  Success: $SUCCESS"
echo "  Failed: $FAILED"
