#!/bin/bash
# Run tests with and without checkpoint forking

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}=== Running Fork Demo Tests ===${NC}"
echo ""

# Determine sui binary
if [ -x "../../../target/release/sui" ]; then
    SUI_BIN="../../../target/release/sui"
    echo -e "${GREEN}Using locally built sui binary${NC}"
elif [ -x "../../../target/debug/sui" ]; then
    SUI_BIN="../../../target/debug/sui"
    echo -e "${GREEN}Using locally built sui binary (debug)${NC}"
else
    SUI_BIN="sui"
    echo -e "${YELLOW}Using system sui binary${NC}"
fi
echo ""

# Test 1: Normal tests without fork
echo -e "${GREEN}Test 1: Running tests WITHOUT checkpoint fork${NC}"
echo "Command: $SUI_BIN move test"
echo ""
"$SUI_BIN" move test
echo ""
echo -e "${GREEN}✓ Normal tests passed${NC}"
echo ""

# Test 2: With checkpoint fork (if config exists)
if [ -f "config.json" ] && [ -f "object_ids.txt" ]; then
    CHECKPOINT=$(jq -r '.checkpoint // empty' config.json)

    if [ -n "$CHECKPOINT" ]; then
        echo -e "${GREEN}Test 2: Running tests WITH checkpoint fork${NC}"

        # Get RPC URL from active environment
        RPC_URL=$(sui client active-env --json 2>/dev/null | jq -r '.rpc' || echo "https://fullnode.testnet.sui.io:443")

        echo "Checkpoint: $CHECKPOINT"
        echo "RPC URL: $RPC_URL"
        echo "Object IDs file: object_ids.txt"
        echo ""
        echo "Command: $SUI_BIN move test --fork-checkpoint $CHECKPOINT --fork-rpc-url $RPC_URL --object-id-file object_ids.txt"
        echo ""

        "$SUI_BIN" move test \
            --fork-checkpoint "$CHECKPOINT" \
            --fork-rpc-url "$RPC_URL" \
            --object-id-file object_ids.txt

        echo ""
        echo -e "${GREEN}✓ Fork tests passed${NC}"
    else
        echo -e "${YELLOW}Skipping fork test: No checkpoint found in config.json${NC}"
        echo "Run scripts/2_mint_and_setup.sh to set up fork test data"
    fi
else
    echo -e "${YELLOW}Skipping fork test: config.json or object_ids.txt not found${NC}"
    echo "Run scripts/1_deploy.sh and scripts/2_mint_and_setup.sh first"
fi

echo ""
echo -e "${BLUE}=== All Tests Complete ===${NC}"
