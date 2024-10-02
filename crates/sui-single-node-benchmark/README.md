# Sui Single Node Benchmark

This crate contains a binary for performance benchmarking a single Sui node.
Upon running the binary, the node will instantiate a standalone `AuthorityState`, and submit
executable transactions to it in parallel. We then measure the time it takes for it to finish
executing all the transactions.

## Usage
To run the benchmark, you can simply run the following command:
```
cargo run --release --bin sui-single-node-benchmark -- ptb
```
By default, it generates 50,0000 transactions, which can be changed using `--tx-count`. Each transaction will contain an empty PTB without any command (i.e. essentially a nop transaction).

### PTB benchmark workloads
When running the PTB benchmark, one can adjust the workload to stress test different parts
of the execution engine:
- `--num-transfers`: this specifies number of transfers made per transaction. Default to 0.
- `--use-native-transfer`: this is false by default, which means we use Move call to transfer objects. When specified, we will use the native TransferObject command without invoking Move to transfer objects.
- `--num-dynamic-fields`: this specifies number of dynamic fields read by each transaction. Default to 0.
- `--computation`: this specifies computation intensity. An increase by 1 means 100 more loop iterations in Fibonacci computation. Default to 0.

### Publish benchmark workloads
WIP (please refer to smoke_tests to see how its setup)

### Components
By default, the benchmark will use the `AuthorityState::try_execute_immediately` entry function,
which includes the execution layer as well as the interaction with the DB. This is equivalent to running:
```
cargo run --release --bin sui-single-node-benchmark -- --component baseline ptb
```
The benchmark supports various component:
- `baseline`: this is the default component, which includes the execution layer as well as the interaction with the DB.
- `execution-only`: compared to baseline, this doesn't interact with RocksDB at all, and only does execution.
- `with-tx-manager`: on top of baseline, it schedules transactions into the transaction manager, instead of executing them immediately. It also goes through the execution driver.
- `validator-without-consensus`: in this mode, transactions are sent to the `handle_certificate` GRPC entry point of the validator service. On top of `with-tx-manager`, it also includes the verification of the certificate.
- `validator-with-fake-consensus`: in this mode, on top of `validator-without-consensus`, it also submits the transactions to a simple consensus layer, which sequences transactions in the order as it receives it directly back to the store. It covers part of the cost in consensus handler. The commit size can be controlled with `--checkpoint-size`.
- `txn-signing`: in this mode, instead of executing transactions, we only benchmark transactions signing.
- `checkpoint-executor`: in this mode, we benchmark how long it takes for the checkpoint executor to execute all checkpoints (i.e. all transactions in them) for the entire epoch. We first construct transactions and effects by actually executing them, and revert them as if they were never executed, construct checkpoints using the results, and then start the checkpoint executor. The size of checkpoints can be controlled with `--checkpoint-size`.


### Profiling
If you are interested in profiling Sui, you can start the benchmark, wait for it to print out "Started execution...", and then attach a profiler to the process.
