#!/bin/bash
# Run tests with fork testing

set -e

RPC_URL="https://fullnode.testnet.sui.io:443"
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
if [ -x "../../../target/debug/sui" ]; then
    SUI_BIN="../../../target/debug/sui"
    echo -e "${GREEN}Using locally built sui binary (debug)${NC}"
elif [ -x "../../../target/release/sui" ]; then
    SUI_BIN="../../../target/release/sui"
    echo -e "${GREEN}Using locally built sui binary (release)${NC}"
else
    SUI_BIN="sui"
    echo -e "${YELLOW}Using system sui binary${NC}"
fi
echo ""

# Test with fork (if config exists)
if [ -f "config.json" ] && [ -f "object_ids.txt" ]; then
    echo -e "${GREEN}Test: Running tests WITH fork state${NC}"
    echo "RPC URL: $RPC_URL"
    echo "Object IDs file: object_ids.txt"
    echo ""
    echo "Command: $SUI_BIN move test --fork-rpc-url $RPC_URL --object-id-file object_ids.txt"
    echo ""

    # Add a small delay to ensure the transaction is finalized
    echo "Waiting 5 seconds to ensure transaction is finalized..."
    sleep 5
    echo ""

    "$SUI_BIN" move test \
        --fork-rpc-url "$RPC_URL" \
        --object-id-file object_ids.txt

    echo ""
    echo -e "${GREEN}✓ Fork tests passed${NC}"
else
    echo -e "${YELLOW}Skipping fork test: config.json or object_ids.txt not found${NC}"
    echo "Run scripts/1_deploy.sh and scripts/2_mint_and_setup.sh first"
fi

echo ""
echo -e "${BLUE}=== All Tests Complete ===${NC}"
