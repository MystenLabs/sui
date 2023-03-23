# sui-rpc-loadgen: Load Generator for SUI RPC Servers

`sui-rpc-loadgen`  is a utility that facilitates the generation of read and write loads on single or multiple Sui RPC servers. Its primary functions include performance testing and data correctness verification.

## Features
- **Easily extendable** to support any read/write endpoint
- **Concurrent load generation** with multiple threads,  making it suitable for load testing high-traffic RPC servers.
- **Cross-verifying** results across multiple RPC Servers, ensuring data consistency and accuracy.
- **Performance comparison** between vanilla Full node RPC and Enhanced Full node RPC  

## Getting Started

Run the following command to see available commands:

```bash
cargo run --bin sui-rpc-loadgen -- -h
```

### Example 1: Get All Checkpoints

The following command initiates a single thread (num-threads == 1) to retrieve all checkpoints from the beginning (sequence 0) to the latest, executing the operation exactly once (repeat == 0):
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9124" --num-threads 1 get-checkpoints -start 0 --repeat 0 --interval_in_ms 0
```
This command is equivalent to the simplified version below:
```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" "http://127.0.0.1:9124" --num-threads 1 get-checkpoints
```
Both commands achieve the same outcome: fetching all checkpoints using one thread, without repeating the operation.

**Note** you must put `--num-threads ` after the urls, otherwise the command will not be parsed correctly

### Example 2: (WIP) Execute PaySui Transaction

```bash
cargo run --bin sui-rpc-loadgen -- --urls "http://127.0.0.1:9000" --num-threads 1 pay-sui --repeat 100
```
**NOTE**: right now `pay-sui` only supports 1 thread but multi-threading support can be added pretty easily by assigning different gas coins to different threads