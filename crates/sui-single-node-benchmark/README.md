# Sui Single Node Benchmark

This crate contains a binary for performance benchmarking a single Sui node.
Upon running the binary, the node will instantiate a standalone `AuthorityState`, and submit
executable transactions to it in parallel. We then measure the time it takes for it to finish
executing all the transactions.

## Usage
There are two modes to benchmark: `move` and `no-move`. `move` mode benchmarks the performance
of executing Move transactions, while `no-move` mode benchmarks the performance of executing
non-Move transactions.

To run the benchmark, you can simply run the following command:
```
cargo run --release --bin sui-single-node-benchmark -- move
```
or
```
cargo run --release --bin sui-single-node-benchmark -- no-move

```
By default, it generates 100,0000 transactions, which can be changed using --tx-count.

### Move benchmark workloads
When running the Move benchmark, one can adjust the workload to stress test different parts
of the execution engine:
- `--num-input-objects`: this specifies number of address owned input objects read/mutated by each transaction. Default to 2.
- `--num--dynamic-fields`: this specifies number of dynamic fields read by each transaction. Default to 0.
- `--computation`: this specifies computation intensity. An increase by 1 means 100 more loop iterations in Fibonacci computation. Default to 0.

### Components
By default, the benchmark will use the `AuthorityState::try_execute_immediately` entry function,
which includes the execution layer as well as the interaction with the DB.
When `--end-to-end` option is specified, the benchmark will use the `ValidatorService::execute_certificate_for_testing` entry function,
which covers the full flow of a validator processing a certificate, including
certificate verification as well as transaction manager. It will also submit the transactions
to a dummy consensus layer, which does nothing.

### Profiling
If you are interested in profiling Sui, you can start the benchmark, wait for it to print out "Started execution...", and then attach a profiler to the process.


## Caveat / Future Work
1. More knobs will be added to the benchmark to allow more fine-grained control over the workload. For example, number of objects written.
2. Plan to cover more components eventually, such as the checkpoint builder and checkpoint executor.