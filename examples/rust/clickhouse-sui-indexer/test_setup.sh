#!/bin/bash

# ClickHouse Sui Indexer Test Setup Script

set -e

echo "🚀 Setting up ClickHouse Sui Indexer test environment..."

# Start ClickHouse container
echo "📦 Starting ClickHouse container..."
docker run -d \
  --name clickhouse-server \
  --ulimit nofile=262144:262144 \
  -p 8123:8123 \
  -p 9000:9000 \
  -e CLICKHOUSE_PASSWORD=changeme \
  clickhouse/clickhouse-server:latest

# Wait for ClickHouse to be ready
echo "⏳ Waiting for ClickHouse to be ready..."
sleep 5

# Test connection
echo "🔍 Testing ClickHouse connection..."
until docker exec clickhouse-server clickhouse-client --query "SELECT 1" > /dev/null 2>&1; do
    echo "Waiting for ClickHouse..."
    sleep 2
done

echo "✅ ClickHouse is ready!"

# Set environment variable
export CLICKHOUSE_URL="http://localhost:8123"

echo "🌍 Environment variable set: CLICKHOUSE_URL=$CLICKHOUSE_URL"

# Show next steps
echo ""
echo "🎯 Next steps:"
echo "1. Set the environment variable:"
echo "   export CLICKHOUSE_URL=\"http://localhost:8123\""
echo ""
echo "2. Run the indexer:"
echo "   cargo run -- --rpc-url https://fullnode.mainnet.sui.io:443 --first-checkpoint 1000 --last-checkpoint 1010"
echo ""
echo "3. Query data from ClickHouse:"
echo "   docker exec -it clickhouse-server clickhouse-client"
echo "   SELECT * FROM transactions LIMIT 10;"
echo ""
echo "4. Stop ClickHouse when done:"
echo "   docker stop clickhouse-server && docker rm clickhouse-server"
echo ""
echo "🎉 Setup complete! You can now run the ClickHouse Sui indexer."