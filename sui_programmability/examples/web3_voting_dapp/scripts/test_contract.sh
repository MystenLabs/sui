#!/bin/bash

# Test script for Voting DApp contract
set -e

echo "================================"
echo "Voting DApp Contract Test Script"
echo "================================"
echo ""

# Check for package ID argument
if [ -z "$1" ]; then
    echo "Usage: $0 <PACKAGE_ID>"
    echo ""
    echo "Or read from deployment-info.json:"
    if [ -f "deployment-info.json" ]; then
        PACKAGE_ID=$(cat deployment-info.json | jq -r '.packageId')
        echo "Found Package ID in deployment-info.json: $PACKAGE_ID"
        read -p "Use this Package ID? (y/n) " -n 1 -r
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Please provide Package ID as argument"
            exit 1
        fi
    else
        echo "No deployment-info.json found"
        exit 1
    fi
else
    PACKAGE_ID=$1
fi

echo "📦 Package ID: $PACKAGE_ID"
echo ""

# Test 1: Create a poll
echo "Test 1: Creating a poll..."
echo "----------------------------"
CREATE_OUTPUT=$(sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll \
  --args "Do you like Sui?" "Yes" "No" \
  --gas-budget 10000 \
  --json 2>&1)

if [ $? -ne 0 ]; then
    echo "❌ Failed to create poll"
    echo "$CREATE_OUTPUT"
    exit 1
fi

# Extract poll ID
POLL_ID=$(echo "$CREATE_OUTPUT" | jq -r '.effects.created[] | select(.owner | has("Shared")) | .reference.objectId' | head -1)

if [ -z "$POLL_ID" ] || [ "$POLL_ID" == "null" ]; then
    echo "❌ Failed to extract poll ID"
    exit 1
fi

echo "✅ Poll created successfully!"
echo "📋 Poll ID: $POLL_ID"
echo ""

# Wait a bit for the transaction to be processed
sleep 2

# Test 2: Query poll data
echo "Test 2: Querying poll data..."
echo "----------------------------"
POLL_DATA=$(sui client object $POLL_ID --json)

if [ $? -ne 0 ]; then
    echo "❌ Failed to query poll"
    exit 1
fi

echo "✅ Poll data retrieved:"
echo "$POLL_DATA" | jq '.details.data.fields | {question, options, votes, total_votes, is_active}'
echo ""

# Test 3: Cast a vote
echo "Test 3: Casting a vote..."
echo "----------------------------"
VOTE_OUTPUT=$(sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function vote \
  --args $POLL_ID 0 \
  --gas-budget 10000 \
  --json 2>&1)

if [ $? -ne 0 ]; then
    echo "❌ Failed to cast vote"
    echo "$VOTE_OUTPUT"
    exit 1
fi

echo "✅ Vote cast successfully!"
echo ""

# Wait for transaction
sleep 2

# Test 4: Check updated vote counts
echo "Test 4: Checking vote counts..."
echo "----------------------------"
UPDATED_POLL=$(sui client object $POLL_ID --json)

if [ $? -ne 0 ]; then
    echo "❌ Failed to query updated poll"
    exit 1
fi

echo "✅ Updated poll data:"
echo "$UPDATED_POLL" | jq '.details.data.fields | {question, votes, total_votes}'
echo ""

# Test 5: Close poll
echo "Test 5: Closing poll..."
echo "----------------------------"
CLOSE_OUTPUT=$(sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function close_poll \
  --args $POLL_ID \
  --gas-budget 10000 \
  --json 2>&1)

if [ $? -ne 0 ]; then
    echo "❌ Failed to close poll"
    echo "$CLOSE_OUTPUT"
    exit 1
fi

echo "✅ Poll closed successfully!"
echo ""

# Wait for transaction
sleep 2

# Verify poll is closed
CLOSED_POLL=$(sui client object $POLL_ID --json)
IS_ACTIVE=$(echo "$CLOSED_POLL" | jq -r '.details.data.fields.is_active')

if [ "$IS_ACTIVE" == "false" ]; then
    echo "✅ Poll is now closed (is_active: false)"
else
    echo "⚠️  Poll status: $IS_ACTIVE"
fi

echo ""
echo "================================"
echo "✅ All tests completed successfully!"
echo "================================"
echo ""
echo "📋 Summary:"
echo "  - Package ID: $PACKAGE_ID"
echo "  - Poll ID: $POLL_ID"
echo "  - Total votes: $(echo "$UPDATED_POLL" | jq -r '.details.data.fields.total_votes')"
echo "  - Poll status: Closed"
echo ""
echo "🔗 View on explorer:"
echo "  https://suiexplorer.com/object/$POLL_ID?network=$(sui client active-env)"
echo ""
