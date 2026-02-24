#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script to check SUI balance for an address
# Usage: ./check-balance.sh <address> [network]

set -e

ADDRESS=$1
NETWORK=${2:-devnet}

if [ -z "$ADDRESS" ]; then
    echo "Usage: $0 <address> [network]"
    echo "  address: Sui address to check"
    echo "  network: devnet, testnet, or mainnet (default: devnet)"
    exit 1
fi

echo "Checking balance for address: $ADDRESS"
echo "Network: $NETWORK"
echo ""

# Check if sui CLI is available
if ! command -v sui &> /dev/null; then
    echo "Error: sui CLI not found. Please install it first."
    exit 1
fi

# Get balance using sui client
sui client gas --address "$ADDRESS" --network "$NETWORK"

echo ""
echo "✓ Balance check complete"
