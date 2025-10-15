#!/bin/bash
# Mint tokens to USER1 and save object IDs for fork testing

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "=== Minting Demo Coins and Setting Up Fork Test Data ==="
echo ""

# Check for config file
if [ ! -f "config.json" ]; then
    echo "Error: config.json not found. Please run 1_deploy.sh first."
    exit 1
fi

# Load configuration
PACKAGE_ID=$(jq -r '.packageId' config.json)
TREASURY_ID=$(jq -r '.treasuryCapId' config.json)

# Define USER1 address (you can change this to any address)
USER1_ADDRESS="0x1111111111111111111111111111111111111111111111111111111111111111"

echo "Package ID: $PACKAGE_ID"
echo "Treasury Cap ID: $TREASURY_ID"
echo "Minting to USER1: $USER1_ADDRESS"
echo ""

# Mint 1,000,000 tokens to USER1
echo "Minting 1,000,000 DEMO tokens to USER1..."
MINT_OUTPUT=$(sui client call \
    --package "$PACKAGE_ID" \
    --module demo_coin \
    --function mint \
    --args "$TREASURY_ID" 1000000 "$USER1_ADDRESS" \
    --gas-budget 10000000 \
    --json)

# Get the current checkpoint
CHECKPOINT=$(sui client execute-signed-transaction --help 2>&1 | grep -o "checkpoint: [0-9]*" | awk '{print $2}' || echo "0")
if [ "$CHECKPOINT" = "0" ]; then
    # Fallback: get latest checkpoint from RPC
    RPC_URL=$(sui client active-env --json | jq -r '.rpc')
    CHECKPOINT=$(curl -s -X POST "$RPC_URL" \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"sui_getLatestCheckpointSequenceNumber","params":[]}' \
        | jq -r '.result')
fi

# Extract created coin object ID
COIN_OBJECT_ID=$(echo "$MINT_OUTPUT" | jq -r '.objectChanges[] | select(.objectType | contains("Coin<")) | select(.owner.AddressOwner == "'$USER1_ADDRESS'") | .objectId')

echo ""
echo "Mint successful!"
echo "Coin Object ID: $COIN_OBJECT_ID"
echo "Checkpoint: $CHECKPOINT"
echo ""

# Save object IDs to file
cat > "$PROJECT_DIR/object_ids.txt" <<EOF
# Object IDs for fork testing
# Coin owned by USER1 ($USER1_ADDRESS)
$COIN_OBJECT_ID
EOF

# Update config with test data
jq --arg checkpoint "$CHECKPOINT" \
   --arg coinId "$COIN_OBJECT_ID" \
   --arg user1 "$USER1_ADDRESS" \
   '. + {checkpoint: $checkpoint, user1CoinId: $coinId, user1Address: $user1}' \
   config.json > config.tmp && mv config.tmp config.json

echo "Object IDs saved to object_ids.txt"
echo "Configuration updated in config.json"
echo ""
echo "Setup complete! You can now run fork tests with:"
echo "  sui move test --fork-checkpoint $CHECKPOINT --fork-rpc-url <RPC_URL> --object-id-file object_ids.txt"
