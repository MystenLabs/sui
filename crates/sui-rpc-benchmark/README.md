# sui-rpc-benchmark: Benchmarking Tool for SUI RPC Performance

`sui-rpc-benchmark` is a benchmarking utility designed to measure performance across different RPC access methods in Sui:
- Direct database reads
- JSON RPC endpoints 
- GraphQL queries

## Usage Examples

```
# Direct database queries:
cargo run --bin sui-rpc-benchmark direct --db-url postgres://postgres:postgres@localhost:5432/sui --concurrency 50 --duration-secs 30

# JSON RPC endpoints:
cargo run --bin sui-rpc-benchmark jsonrpc --endpoint http://127.0.0.1:9000 --concurrency 50 --requests-file requests.jsonl [--methods-to-skip method1,method2]

# GraphQL queries (not fully implemented):
cargo run --bin sui-rpc-benchmark graphql --endpoint http://127.0.0.1:9000/graphql
```

## Options

### Direct Query Benchmark
- `--db-url`: PostgreSQL database URL
- `--concurrency`: Number of concurrent queries (default: 50)
- `--duration-secs`: Optional duration of the benchmark in seconds

### JSON RPC Benchmark
- `--endpoint`: JSON RPC endpoint URL (default: http://127.0.0.1:9000)
- `--concurrency`: Number of concurrent requests (default: 50)
- `--requests-file`: File containing requests in JSONL (JSON Lines) format
- `--duration-secs`: Optional duration limit in seconds
- `--methods-to-skip`: Optional comma-separated list of methods to skip

### GraphQL Benchmark
- `--endpoint`: GraphQL endpoint URL
