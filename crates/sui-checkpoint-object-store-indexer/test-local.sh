#!/bin/bash
set -e

# Configuration
export DATABASE_URL="postgres://localhost:5432/sui_checkpoints"
export OBJECT_STORE_URL="file:///tmp/test-checkpoints"
export RPC_API_URL="https://fullnode.testnet.sui.io:443"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}Setting up local test environment...${NC}"

# Step 1: Check if PostgreSQL is installed
echo -e "${GREEN}1. Checking PostgreSQL installation...${NC}"
if ! command -v psql &>/dev/null; then
  echo -e "${YELLOW}PostgreSQL not found. Install with: brew install postgresql@15${NC}"
  echo -e "${YELLOW}Then start it with: brew services start postgresql@15${NC}"
  exit 1
fi

# Check if PostgreSQL is running
if ! pg_isready &>/dev/null; then
  echo -e "${YELLOW}PostgreSQL is not running. Start it with: brew services start postgresql@15${NC}"
  echo -e "${YELLOW}Or if already installed: pg_ctl -D /opt/homebrew/var/postgresql@15 start${NC}"
  exit 1
fi

# Step 2: Create database if it doesn't exist
echo -e "${GREEN}2. Creating database...${NC}"
createdb sui_checkpoints 2>/dev/null || echo "Database already exists, continuing..."

# Step 2: Create local directory for object store
echo -e "${GREEN}3. Creating local object store directory...${NC}"
mkdir -p /tmp/test-checkpoints
echo "Object store directory: /tmp/test-checkpoints"

# Step 3: Run the indexer
echo -e "${GREEN}4. Running checkpoint indexer...${NC}"
echo "Indexing checkpoints 250656893 to 250656903 from testnet"
echo ""

cargo run -p sui-checkpoint-object-store-indexer -- \
  --database-url "$DATABASE_URL" \
  --object-store-url "$OBJECT_STORE_URL" \
  --rpc-api-url "$RPC_API_URL" \
  --first-checkpoint 250656893 \
  --last-checkpoint 250656903 \
  --compression-level 19

echo ""
echo -e "${GREEN}5. Indexing complete! Checking results...${NC}"

# Step 4: Verify results
echo "Files created in object store:"
ls -lh /tmp/test-checkpoints/

echo ""
echo "Decompressing a sample checkpoint to verify:"
SAMPLE_FILE=$(ls /tmp/test-checkpoints/*.zst | head -1)
if [ -n "$SAMPLE_FILE" ]; then
  zstd -d "$SAMPLE_FILE" -c | head -c 100
  echo "... (truncated)"
  echo ""
  echo "File size: $(ls -lh "$SAMPLE_FILE" | awk '{print $5}')"
fi

echo ""
echo -e "${BLUE}Test complete!${NC}"
echo ""
echo "To clean up:"
echo "  dropdb sui_checkpoints"
echo "  rm -rf /tmp/test-checkpoints"
