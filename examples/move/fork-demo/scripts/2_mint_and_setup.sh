#!/bin/bash
# Mint tokens to USER1 and save object IDs for fork testing

set -e

RPC_URL="https://fullnode.testnet.sui.io:443"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Determine sui binary to use
SUI_BIN="../../../target/debug/sui"
if [ ! -x "$SUI_BIN" ]; then
    SUI_BIN="sui"
    echo "Warning: Using system sui binary (debug build not found)"
fi

echo "=== Minting Demo Coins and Setting Up Fork Test Data ==="
echo "Using sui binary: $SUI_BIN"
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
MINT_OUTPUT=$($SUI_BIN client call \
    --package "$PACKAGE_ID" \
    --module demo_coin \
    --function mint \
    --args "$TREASURY_ID" 1000000 "$USER1_ADDRESS" \
    --gas-budget 10000000 \
    --json)

# Save the output for debugging
echo "$MINT_OUTPUT" > mint_output.json

# Extract the transaction digest from the mint output
TX_DIGEST=$(echo "$MINT_OUTPUT" | jq -r '.digest // .transactionBlockDigest // empty')

echo "Transaction Digest: $TX_DIGEST"
echo ""

# Extract created coin object ID
COIN_OBJECT_ID=$(echo "$MINT_OUTPUT" | jq -r '.objectChanges[] | select((.objectType // "") | contains("Coin<")) | select(.owner.AddressOwner == "'$USER1_ADDRESS'") | .objectId')

# Validate we got the coin object ID
if [ -z "$COIN_OBJECT_ID" ] || [ "$COIN_OBJECT_ID" = "null" ]; then
    echo "Error: Failed to extract coin object ID"
    echo "Check mint_output.json for details"
    exit 1
fi

# Extract all created shared objects from mint output
SHARED_OBJECT_IDS=$(echo "$MINT_OUTPUT" | jq -r '.objectChanges[] | select(.type == "created" and (.owner | type == "object" and has("Shared"))) | .objectId' | tr '\n' ' ')

# Also extract shared objects from publish output if it exists
PUBLISH_SHARED_IDS=""
if [ -f "publish_output.json" ]; then
    PUBLISH_SHARED_IDS=$(jq -r '.objectChanges[] | select(.type == "created" and (.owner | type == "object" and has("Shared"))) | .objectId' publish_output.json | tr '\n' ' ')
    if [ -n "$PUBLISH_SHARED_IDS" ]; then
        SHARED_OBJECT_IDS="$SHARED_OBJECT_IDS $PUBLISH_SHARED_IDS"
    fi
fi

echo "Mint successful!"
echo "Coin Object ID: $COIN_OBJECT_ID"
if [ -n "$SHARED_OBJECT_IDS" ]; then
    echo "Shared Object IDs: $SHARED_OBJECT_IDS"
fi
echo ""

# Extract DEMO_STATE object ID from shared objects
DEMO_STATE_ID=$(echo "$MINT_OUTPUT" | jq -r '.objectChanges[] | select(.type == "created" and (.objectType | type == "string" and contains("DEMO_STATE"))) | .objectId')

# If not found in mint output, check publish output
if [ -z "$DEMO_STATE_ID" ] || [ "$DEMO_STATE_ID" = "null" ]; then
    if [ -f "publish_output.json" ]; then
        DEMO_STATE_ID=$(jq -r '.objectChanges[] | select(.type == "created" and (.objectType | type == "string" and contains("DEMO_STATE"))) | .objectId' publish_output.json)
    fi
fi

# Call add_demo_dynamic function if we have the DEMO_STATE object
if [ -n "$DEMO_STATE_ID" ] && [ "$DEMO_STATE_ID" != "null" ]; then
    echo "Calling add_demo_dynamic with DEMO_STATE: $DEMO_STATE_ID"
    ADD_DYNAMIC_OUTPUT=$($SUI_BIN client call \
        --package "$PACKAGE_ID" \
        --module demo_coin \
        --function add_demo_dynamic \
        --args "$DEMO_STATE_ID" \
        --gas-budget 10000000 \
        --json)
    
    # Save the output for debugging
    echo "$ADD_DYNAMIC_OUTPUT" > add_dynamic_output.json
    
    # Extract transaction digest
    ADD_DYNAMIC_TX=$(echo "$ADD_DYNAMIC_OUTPUT" | jq -r '.digest // .transactionBlockDigest // empty')
    echo "add_demo_dynamic Transaction Digest: $ADD_DYNAMIC_TX"
    echo ""
else
    echo "Warning: DEMO_STATE object not found, skipping add_demo_dynamic call"
    echo ""
fi

# Save object IDs to file
cat > "$PROJECT_DIR/object_ids.txt" <<EOF
# Object IDs for fork testing
# Coin owned by USER1 ($USER1_ADDRESS)
$COIN_OBJECT_ID
EOF

# Add shared objects if any exist
if [ -n "$SHARED_OBJECT_IDS" ]; then
    echo "# Created shared objects" >> "$PROJECT_DIR/object_ids.txt"
    for SHARED_ID in $SHARED_OBJECT_IDS; do
        if [ -n "$SHARED_ID" ]; then
            echo "$SHARED_ID" >> "$PROJECT_DIR/object_ids.txt"
        fi
    done
fi

# Update config with test data
CONFIG_UPDATE=$(jq --arg coinId "$COIN_OBJECT_ID" \
   --arg user1 "$USER1_ADDRESS" \
   --arg demoStateId "$DEMO_STATE_ID" \
   '. + {user1CoinId: $coinId, user1Address: $user1, demoStateId: $demoStateId}' \
   config.json)
echo "$CONFIG_UPDATE" > config.json

echo "Object IDs saved to object_ids.txt"
echo "Configuration updated in config.json"
echo ""
echo "Setup complete! You can now run fork tests with:"
echo "  sui move test --fork-rpc-url $RPC_URL --object-id-file object_ids.txt"
