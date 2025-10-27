#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Deploy the demo coin contract to testnet

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Determine sui binary to use
SUI_BIN="../../../target/debug/sui"
if [ ! -x "$SUI_BIN" ]; then
    SUI_BIN="sui"
    echo "Warning: Using system sui binary (debug build not found)"
fi

echo "=== Deploying Demo Coin to Testnet ==="
echo "Using sui binary: $SUI_BIN"
echo ""

# Check if sui client is configured
if ! $SUI_BIN client active-address &>/dev/null; then
    echo "Error: Sui client not configured. Please run '$SUI_BIN client' first."
    exit 1
fi

ACTIVE_ADDRESS=$($SUI_BIN client active-address)
echo "Active address: $ACTIVE_ADDRESS"
echo ""

# Build the package
echo "Building package..."
$SUI_BIN move build
echo ""

# Publish the package
echo "Publishing package..."
PUBLISH_OUTPUT=$($SUI_BIN client publish --gas-budget 100000000 --json)

# Save the output for debugging
echo "$PUBLISH_OUTPUT" > publish_output.json

# Extract package ID and treasury cap ID
PACKAGE_ID=$(echo "$PUBLISH_OUTPUT" | jq -r '.objectChanges[] | select(.type == "published") | .packageId')
TREASURY_ID=$(echo "$PUBLISH_OUTPUT" | jq -r '.objectChanges[] | select((.objectType // "") | contains("TreasuryCap")) | .objectId')

echo "Package ID: $PACKAGE_ID"
echo "Treasury Cap ID: $TREASURY_ID"

# Verify we got valid IDs
if [ -z "$PACKAGE_ID" ] || [ "$PACKAGE_ID" = "null" ]; then
    echo "Error: Failed to extract Package ID"
    echo "Check publish_output.json for details"
    exit 1
fi

if [ -z "$TREASURY_ID" ] || [ "$TREASURY_ID" = "null" ]; then
    echo "Error: Failed to extract Treasury Cap ID"
    echo "Check publish_output.json for details"
    exit 1
fi
echo ""

# Save to config file
cat > "$PROJECT_DIR/config.json" <<EOF
{
  "packageId": "$PACKAGE_ID",
  "treasuryCapId": "$TREASURY_ID",
  "adminAddress": "$ACTIVE_ADDRESS"
}
EOF

echo "Configuration saved to config.json"
echo ""
echo "Deployment successful!"
