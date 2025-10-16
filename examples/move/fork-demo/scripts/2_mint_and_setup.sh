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
echo "RPC URL: $RPC_URL"

# Query the transaction to get its checkpoint
if [ -n "$TX_DIGEST" ]; then
    CHECKPOINT=$(curl -s -X POST "$RPC_URL" \
        -H 'Content-Type: application/json' \
        -d '{
            "jsonrpc":"2.0",
            "id":1,
            "method":"sui_getTransactionBlock",
            "params":["'"$TX_DIGEST"'", {"showEffects": true}]
        }' | jq -r '.result.checkpoint // empty')
fi

# Fallback: get latest checkpoint if transaction query failed
if [ -z "$CHECKPOINT" ] || [ "$CHECKPOINT" = "null" ]; then
    echo "Warning: Could not get checkpoint from transaction, using latest checkpoint..."
    CHECKPOINT=$(curl -s -X POST "$RPC_URL" \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"sui_getLatestCheckpointSequenceNumber","params":[]}' \
        | jq -r '.result // empty')
fi

# Extract created coin object ID
COIN_OBJECT_ID=$(echo "$MINT_OUTPUT" | jq -r '.objectChanges[] | select((.objectType // "") | contains("Coin<")) | select(.owner.AddressOwner == "'$USER1_ADDRESS'") | .objectId')

echo ""

# Validate we got the necessary data
if [ -z "$COIN_OBJECT_ID" ] || [ "$COIN_OBJECT_ID" = "null" ]; then
    echo "Error: Failed to extract coin object ID"
    echo "Check mint_output.json for details"
    exit 1
fi

if [ -z "$CHECKPOINT" ] || [ "$CHECKPOINT" = "null" ]; then
    echo "Error: Failed to get checkpoint number"
    echo "Check mint_output.json for details"
    exit 1
fi

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
