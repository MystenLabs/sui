# ClickHouse Sui Indexer

A simple example of how to build a custom Sui indexer that writes transaction data to ClickHouse.

## Quick Start

### 1. Start ClickHouse

```bash
docker run -d --name clickhouse-dev -p 8123:8123 -p 9000:9000 --ulimit nofile=262144:262144 clickhouse/clickhouse-server
```

### 2. Set up database user

```bash
docker exec clickhouse-dev clickhouse-client --query "CREATE USER IF NOT EXISTS dev IDENTIFIED WITH no_password"
docker exec clickhouse-dev clickhouse-client --query "GRANT CREATE, INSERT, SELECT, ALTER, UPDATE, DELETE ON *.* TO dev"
```

### 3. Run the indexer

```bash
cargo run -- --remote-store-url https://checkpoints.testnet.sui.io --last-checkpoint=10
```

That's it! The indexer will:
- Create the necessary tables automatically
- Fetch checkpoints from the Sui testnet
- Write transaction data to ClickHouse

## Verify Data

Check that data was written:

```bash
docker exec clickhouse-dev clickhouse-client --user=dev --query "SELECT COUNT(*) FROM transactions"
docker exec clickhouse-dev clickhouse-client --user=dev --query "SELECT * FROM transactions LIMIT 5"
```

## Clean Up

Stop and remove the ClickHouse container:

```bash
docker stop clickhouse-dev && docker rm clickhouse-dev
```

## What This Example Shows

- **Custom Store Implementation**: How to implement the `Store` trait for ClickHouse
- **Concurrent Pipeline**: Uses the concurrent pipeline for better pruning and watermark testing
- **Watermark Management**: Tracking indexer progress with committer, reader, and pruner watermarks
- **Transaction Processing**: Extracting and storing transaction digests from checkpoints
- **Simple Setup**: Minimal configuration for local development

## Architecture

```
Sui Network → Checkpoints → Concurrent Pipeline → ClickHouse Store → ClickHouse DB
```

The indexer uses a concurrent pipeline that processes checkpoints out-of-order with separate reader, committer, and pruner components. This is ideal for testing watermark functionality and pruning behavior.