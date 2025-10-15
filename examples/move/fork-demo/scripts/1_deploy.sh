#!/bin/bash
# Deploy the demo coin contract to testnet

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "=== Deploying Demo Coin to Testnet ==="
echo ""

# Check if sui client is configured
if ! sui client active-address &>/dev/null; then
    echo "Error: Sui client not configured. Please run 'sui client' first."
    exit 1
fi

ACTIVE_ADDRESS=$(sui client active-address)
echo "Active address: $ACTIVE_ADDRESS"
echo ""

# Build the package
echo "Building package..."
sui move build
echo ""

# Publish the package
echo "Publishing package..."
PUBLISH_OUTPUT=$(sui client publish --gas-budget 100000000 --json)

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
