#!/bin/bash

# Merge Sui Gas Objects Script using PTB (Programmable Transaction Block)
# This script takes all gas objects owned by an address and merges them efficiently in a single transaction

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to display usage
usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  -a, --address ADDRESS    Specify the address (optional, uses active address if not provided)"
    echo "  -e, --env ENV            Specify the environment (optional, uses active env if not provided)"
    echo "  -k, --keep NUMBER        Keep this many gas objects separate (default: 1)"
    echo "  -d, --dry-run            Show what would be merged without executing"
    echo "  -b, --batch-size SIZE    Max coins to merge per transaction (default: 50)"
    echo "  -v, --verbose            Show full transaction details (digest, effects, etc)"
    echo "  -h, --help               Display this help message"
    exit 1
}

# Parse command line arguments
ADDRESS=""
ENV=""
KEEP_COUNT=1
DRY_RUN=false
BATCH_SIZE=50
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -a|--address)
            ADDRESS="$2"
            shift 2
            ;;
        -e|--env)
            ENV="$2"
            shift 2
            ;;
        -k|--keep)
            KEEP_COUNT="$2"
            shift 2
            ;;
        -d|--dry-run)
            DRY_RUN=true
            shift
            ;;
        -b|--batch-size)
            BATCH_SIZE="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate batch size
if [ "$BATCH_SIZE" -gt 100 ]; then
    echo -e "${YELLOW}Batch size too large, setting to maximum of 100${NC}"
    BATCH_SIZE=100
fi

# Set environment if specified
if [ -n "$ENV" ]; then
    echo -e "${YELLOW}Switching to environment: $ENV${NC}"
    sui client switch --env "$ENV"
fi

# Get the active address if not specified
if [ -z "$ADDRESS" ]; then
    ADDRESS=$(sui client active-address)
    echo -e "${GREEN}Using active address: $ADDRESS${NC}"
else
    echo -e "${GREEN}Using specified address: $ADDRESS${NC}"
fi

# Get all gas objects
echo -e "${YELLOW}Fetching gas objects...${NC}"
GAS_OBJECTS=$(sui client gas --json "$ADDRESS" 2>/dev/null | jq -r '.[].gasCoinId')

# Count total gas objects
TOTAL_COUNT=$(echo "$GAS_OBJECTS" | wc -l | tr -d ' ')

if [ "$TOTAL_COUNT" -le "$KEEP_COUNT" ]; then
    echo -e "${GREEN}You have $TOTAL_COUNT gas object(s), which is already at or below the keep count of $KEEP_COUNT.${NC}"
    echo "No merging needed."
    exit 0
fi

# Convert gas objects to array (macOS compatible)
IFS=$'\n'
GAS_ARRAY=($GAS_OBJECTS)
unset IFS

# Keep the first KEEP_COUNT coins and merge the rest
PRIMARY_COIN="${GAS_ARRAY[0]}"
COINS_TO_MERGE=("${GAS_ARRAY[@]:$KEEP_COUNT}")
MERGE_COUNT=$((TOTAL_COUNT - KEEP_COUNT))

echo -e "${YELLOW}Total gas objects: $TOTAL_COUNT${NC}"
echo -e "${YELLOW}Primary coin: $PRIMARY_COIN${NC}"
echo -e "${YELLOW}Coins to merge: $MERGE_COUNT${NC}"

if [ "$MERGE_COUNT" -eq 0 ]; then
    echo -e "${GREEN}No coins need to be merged based on the criteria.${NC}"
    exit 0
fi

# Display coins to be merged in dry run
if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}Dry run mode - showing coins that would be merged:${NC}"
    if [ "$MERGE_COUNT" -le 10 ]; then
        for coin in "${COINS_TO_MERGE[@]}"; do
            echo "  - $coin"
        done
    else
        echo "  - First 5 coins:"
        for ((i=0; i<5 && i<$MERGE_COUNT; i++)); do
            echo "    - ${COINS_TO_MERGE[$i]}"
        done
        echo "  - ... ($((MERGE_COUNT - 10)) more coins) ..."
        echo "  - Last 5 coins:"
        for ((i=$((MERGE_COUNT - 5)); i<$MERGE_COUNT; i++)); do
            echo "    - ${COINS_TO_MERGE[$i]}"
        done
    fi
    echo -e "${GREEN}Dry run complete. No actual merging performed.${NC}"
    exit 0
fi

# Function to build and execute PTB
execute_ptb_merge() {
    local primary=$1
    shift
    local coins_to_merge=("$@")
    local batch_count=${#coins_to_merge[@]}
    
    # Build the coin array string with proper formatting
    local coin_array=""
    for coin in "${coins_to_merge[@]}"; do
        if [ -z "$coin_array" ]; then
            coin_array="@${coin}"
        else
            coin_array="${coin_array}, @${coin}"
        fi
    done
    
    # Build the PTB command with proper quoting
    # Format: sui client ptb --merge-coins @primary_coin '[array_of_coins]'
    if [ "$VERBOSE" = false ]; then
        echo -ne "${YELLOW}Merging $batch_count coins...${NC}"
        # Redirect output to /dev/null if not verbose, capture only success/failure
        if sui client ptb --merge-coins "@${primary}" "[${coin_array}]" > /dev/null 2>&1; then
            echo -e " ${GREEN}✓${NC}"
            return 0
        else
            echo -e " ${RED}✗${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}Executing PTB to merge $batch_count coins into primary coin...${NC}"
        if sui client ptb --merge-coins "@${primary}" "[${coin_array}]"; then
            echo -e "${GREEN}Successfully merged batch!${NC}"
            return 0
        else
            echo -e "${RED}Failed to merge batch${NC}"
            return 1
        fi
    fi
}

# Process merges in batches if necessary
TOTAL_BATCHES=$(( (MERGE_COUNT + BATCH_SIZE - 1) / BATCH_SIZE ))
MERGED_TOTAL=0

if [ "$TOTAL_BATCHES" -gt 1 ]; then
    echo -e "${YELLOW}Processing $MERGE_COUNT coins in $TOTAL_BATCHES batches of up to $BATCH_SIZE each...${NC}"
fi

# Show progress header if not verbose
if [ "$VERBOSE" = false ] && [ "$TOTAL_BATCHES" -gt 1 ]; then
    echo -e "${YELLOW}Progress:${NC}"
fi

for ((batch=0; batch<$TOTAL_BATCHES; batch++)); do
    START_IDX=$((batch * BATCH_SIZE))
    END_IDX=$((START_IDX + BATCH_SIZE))
    if [ "$END_IDX" -gt "$MERGE_COUNT" ]; then
        END_IDX=$MERGE_COUNT
    fi
    
    BATCH_SIZE_ACTUAL=$((END_IDX - START_IDX))
    BATCH_COINS=("${COINS_TO_MERGE[@]:$START_IDX:$BATCH_SIZE_ACTUAL}")
    
    if [ "$VERBOSE" = false ]; then
        # Show batch progress in compact format
        printf "${YELLOW}[%3d/%3d]${NC} " $((batch + 1)) $TOTAL_BATCHES
    else
        if [ "$TOTAL_BATCHES" -gt 1 ]; then
            echo -e "${YELLOW}Processing batch $((batch + 1))/$TOTAL_BATCHES (coins $((START_IDX + 1)) to $END_IDX)...${NC}"
        fi
    fi
    
    if execute_ptb_merge "$PRIMARY_COIN" "${BATCH_COINS[@]}"; then
        MERGED_TOTAL=$((MERGED_TOTAL + BATCH_SIZE_ACTUAL))
        if [ "$VERBOSE" = true ]; then
            echo -e "${GREEN}Successfully merged $BATCH_SIZE_ACTUAL coins in this batch!${NC}"
        fi
    else
        if [ "$VERBOSE" = true ]; then
            echo -e "${RED}Failed to merge batch $((batch + 1)). Some coins may already be merged.${NC}"
        fi
        # Try to continue with next batch
    fi
done

echo -e "${GREEN}Merge operation completed! Merged $MERGED_TOTAL coins total.${NC}"

# Show final gas objects count
echo -e "${YELLOW}Fetching updated gas objects...${NC}"
FINAL_COUNT=$(sui client gas --json "$ADDRESS" 2>/dev/null | jq -r '.[].gasCoinId' | wc -l | tr -d ' ')
echo -e "${GREEN}Final gas object count: $FINAL_COUNT${NC}"
echo -e "${GREEN}Reduction: $TOTAL_COUNT → $FINAL_COUNT (merged $((TOTAL_COUNT - FINAL_COUNT)) coins)${NC}"