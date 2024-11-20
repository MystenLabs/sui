# sui-rpc-benchmark: Benchmarking Tool for SUI RPC Performance

`sui-rpc-benchmark` is a benchmarking utility designed to measure and compare performance across different RPC access methods in Sui:

- Direct database reads
- JSON RPC endpoints 
- GraphQL queries

## Overview

The benchmark tool helps evaluate:
- Query latency and throughput
- Resource utilization
- Performance at scale

## Usage Examples

Run benchmarks with:

```
# Direct database queries:
cargo run --release --bin sui-rpc-benchmark direct --num-queries 100 --num-threads 1

# JSON RPC endpoints:
cargo run --release --bin sui-rpc-benchmark jsonrpc --endpoint http://127.0.0.1:9000 --num-queries 100 --num-threads 1

# GraphQL queries:
cargo run --release --bin sui-rpc-benchmark graphql --endpoint http://127.0.0.1:9000/graphql --num-queries 100 --num-threads 1
```



