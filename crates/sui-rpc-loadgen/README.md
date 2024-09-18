# sui-rpc-loadgen: Load Generator for SUI RPC Servers

`sui-rpc-loadgen` is a utility that facilitates the generation of read and write loads on single or multiple Sui RPC servers. Its primary functions include performance testing and data correctness verification.

## Features

- **Easily extendable** to support any read/write endpoint
- **Concurrent load generation** with multiple threads, making it suitable for load testing high-traffic RPC servers.
- **Cross-verifying** results across multiple RPC Servers, ensuring data consistency and accuracy.
- **Performance comparison** between vanilla Full node RPC and Enhanced Full node RPC

## Getting Started

Run the following command to see available commands:

```bash
cargo run --bin sui-rpc-loadgen -- -h
```

To try this locally, refer to the [docs](https://docs.sui.io/guides/developer/getting-started/local-network). Recommend setting `database-url` to an env variable. Note: run `RUST_LOG="consensus=off" cargo run sui -- start --with-faucet --force-regenesis --with-indexer` to rebuild.

### Example 1: Get All Checkpoints

The following command initiates a single thread (num-threads == 1) to retrieve all checkpoints from the beginning (sequence 0) to the latest, executing the operation exactly once (repeat == 0):

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9124" --num-threads 1 get-checkpoints --start 0 --repeat 0 --interval-in-ms 0
```

This command is equivalent to the simplified version below:

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9124" --num-threads 1 get-checkpoints
```

Both commands achieve the same outcome: fetching all checkpoints using one thread, without repeating the operation.

By default, this command also verify all the transactions in the checkpoint, specify `--skip-verify-transactions` to disable fetching transactions. Note that this must used with `--skip-verify-objects` as we do need to fetch transactions to get objects for the checkpoint.

**Note** you must put `--num-threads ` after the urls, otherwise the command will not be parsed correctly

### Example 2: (WIP) Execute PaySui Transaction

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" --num-threads 1 pay-sui --repeat 100
```

**NOTE**: right now `pay-sui` only supports 1 thread but multi-threading support can be added pretty easily by assigning different gas coins to different threads

### Example 3: Query Transaction Blocks

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 4 query-transaction-blocks --address-type from
```

### Multi Get Transaction Blocks
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 4 multi-get-transaction-blocks
```

### Multi Get Objects

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 4 multi-get-objects
```

### Get Object
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 2 get-object --chunk-size 20
```

### Get All Balances
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 2 get-all-balances --chunk-size 20
```


### Get Reference Gas Price
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9000" --num-threads 2 get-reference-gas-price --num-chunks-per-thread 10
```

# Useful commands

```bash
cat sui-rpc-loadgen.b844f547-d354-4871-b958-1ea3fe23a0a8.log.2023-03-23 | awk '/Finished processing/{print $7}' | sort -n | uniq | awk 'BEGIN{last=0}{for(i=last+1;i<$1;i++) print i; last=$1} END{print last}' | tee missing_numbers.txt && wc -l missing_numbers.txt
```

Checks which checkpoints among threads have not been processed yet. The last one should be the largest checkpoint being processed.

`wc -l missing_numbers.txt` - counts how many checkpoints to go
