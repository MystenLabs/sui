#!/bin/bash

# Deployment script for Voting DApp
set -e

echo "==================================="
echo "Sui Voting DApp Deployment Script"
echo "==================================="
echo ""

# Check if sui CLI is installed
if ! command -v sui &> /dev/null; then
    echo "❌ Error: sui CLI is not installed"
    echo "Please install it first: https://docs.sui.io/build/install"
    exit 1
fi

echo "✅ Sui CLI found"

# Check current network
CURRENT_ENV=$(sui client active-env 2>/dev/null || echo "none")
echo "📡 Current network: $CURRENT_ENV"

# Check if user wants to continue
echo ""
read -p "Do you want to continue with this network? (y/n) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Deployment cancelled"
    exit 0
fi

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Navigate to project directory
cd "$PROJECT_DIR"

echo ""
echo "📦 Building Move package..."
sui move build

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "✅ Build successful"
echo ""
echo "🚀 Publishing package..."
echo ""

# Publish the package
PUBLISH_OUTPUT=$(sui client publish --gas-budget 50000 --json 2>&1)

if [ $? -ne 0 ]; then
    echo "❌ Publish failed"
    echo "$PUBLISH_OUTPUT"
    exit 1
fi

# Parse the package ID from JSON output
PACKAGE_ID=$(echo "$PUBLISH_OUTPUT" | jq -r '.effects.created[] | select(.owner == "Immutable") | .reference.objectId' | head -1)

if [ -z "$PACKAGE_ID" ] || [ "$PACKAGE_ID" == "null" ]; then
    echo "❌ Failed to extract package ID"
    echo "Output: $PUBLISH_OUTPUT"
    exit 1
fi

echo ""
echo "✅ Package published successfully!"
echo ""
echo "=================================="
echo "📋 Deployment Information"
echo "=================================="
echo "Package ID: $PACKAGE_ID"
echo "Network: $CURRENT_ENV"
echo ""
echo "🔗 Explorer URL:"
echo "https://suiexplorer.com/object/$PACKAGE_ID?network=$CURRENT_ENV"
echo ""
echo "=================================="
echo "📝 Next Steps"
echo "=================================="
echo "1. Update frontend/app.js with Package ID:"
echo "   const PACKAGE_ID = '$PACKAGE_ID';"
echo ""
echo "2. Test the contract with CLI:"
echo "   sui client call --package $PACKAGE_ID --module voting --function create_poll \\"
echo "     --args \"Do you like Sui?\" \"Yes\" \"No\" --gas-budget 10000"
echo ""
echo "3. Run frontend:"
echo "   cd frontend && npm install && npm run dev"
echo ""

# Save deployment info
cat > "$PROJECT_DIR/deployment-info.json" << EOF
{
  "packageId": "$PACKAGE_ID",
  "network": "$CURRENT_ENV",
  "deployedAt": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "deployer": "$(sui client active-address)"
}
EOF

echo "💾 Deployment info saved to deployment-info.json"
echo ""
echo "✨ Deployment complete!"
