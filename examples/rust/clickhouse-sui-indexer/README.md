# ClickHouse Sui Indexer

This example demonstrates how to create a custom Sui indexer that writes transaction digest data to ClickHouse instead of the default PostgreSQL database.

## Features

- Custom ClickHouse store implementation
- Transaction digest indexing with checkpoint sequence numbers
- Efficient bulk inserts using ClickHouse's native client
- Proper watermark management for indexer state

## Prerequisites

You need a running ClickHouse instance. The easiest way is to use Docker:

### Quick Start with Docker

1. **Start ClickHouse server:**
   ```bash
   docker run -d \
     --name clickhouse-server \
     --ulimit nofile=262144:262144 \
     -p 8123:8123 \
     -p 9000:9000 \
     -e CLICKHOUSE_PASSWORD=changeme \
     clickhouse/clickhouse-server:latest
   ```

2. **Verify it's running:**
   ```bash
   docker exec -it clickhouse-server clickhouse-client
   ```

3. **Set environment variable:**
   ```bash
   export CLICKHOUSE_URL="http://localhost:8123"
   ```

## Database Schema

The indexer creates two tables automatically:

### `watermarks` table
- `pipeline_name` (String): Name of the indexing pipeline
- `epoch_hi_inclusive` (UInt64): Latest epoch processed
- `checkpoint_hi_inclusive` (UInt64): Latest checkpoint processed
- `tx_hi` (UInt64): Latest transaction processed
- `timestamp_ms_hi_inclusive` (UInt64): Timestamp of latest processed data
- `reader_lo` (UInt64): Lower bound for readers
- `pruner_hi` (UInt64): Upper bound for pruner
- `pruner_timestamp` (DateTime64): When pruner last updated

### `transactions` table
- `checkpoint_sequence_number` (UInt64): Sui checkpoint sequence number
- `transaction_digest` (String): Transaction digest hash
- `indexed_at` (DateTime64): When the record was inserted

## Running the Indexer

1. **Make sure ClickHouse is running** (see Prerequisites above)

2. **Set the ClickHouse URL** (defaults to `http://localhost:8123`):
   ```bash
   export CLICKHOUSE_URL="http://localhost:8123"
   ```

3. **Run the indexer:**
   ```bash
   cargo run -- \
     --rpc-url https://fullnode.mainnet.sui.io:443 \
     --first-checkpoint 1000 \
     --last-checkpoint 1100
   ```

## Querying Data

Once the indexer is running, you can query the ClickHouse database:

```bash
# Connect to ClickHouse
docker exec -it clickhouse-server clickhouse-client

# Query transaction count by checkpoint
SELECT checkpoint_sequence_number, COUNT(*) as tx_count 
FROM transactions 
GROUP BY checkpoint_sequence_number 
ORDER BY checkpoint_sequence_number;

# Query recent transactions
SELECT * FROM transactions 
ORDER BY indexed_at DESC 
LIMIT 10;

# Query watermark status
SELECT * FROM watermarks;
```

## Configuration

The indexer accepts the same command-line arguments as other Sui indexers:

- `--rpc-url`: Sui RPC endpoint
- `--first-checkpoint`: Starting checkpoint (optional)
- `--last-checkpoint`: Ending checkpoint (optional)
- `--checkpoint-buffer-size`: Checkpoint processing buffer size
- `--max-concurrent-requests`: Maximum concurrent RPC requests

## Stopping

To stop the indexer and ClickHouse:

```bash
# Stop the indexer with Ctrl+C

# Stop ClickHouse container
docker stop clickhouse-server
docker rm clickhouse-server
```