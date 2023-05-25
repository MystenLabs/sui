# Narwhal and Consensus

This directory provides an implementation of [Narwhal](https://arxiv.org/pdf/2105.11827.pdf) and [partially synchronous Bullshark](https://arxiv.org/pdf/2209.05633.pdf), a [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph)-based mempool and efficient [BFT](https://en.wikipedia.org/wiki/Byzantine_fault) consensus. The codebase has been designed to be small, efficient, and easy to benchmark and modify. 

This directory uses [fastcrypto](https://github.com/MystenLabs/fastcrypto) as its cryptography library.

## Quick Start
The core protocols are written in Rust, but all benchmarking scripts are written in Python and run with [Fabric](http://www.fabfile.org/).
To deploy and benchmark a testbed of four nodes on your local machine, clone the Sui repo and install the python dependencies:
```
$ git clone https://github.com/mystenlabs/sui.git
$ cd sui/narwhal/benchmark
$ pip install -r requirements.txt
```
You also need to install [Clang](https://clang.llvm.org/) (required by RocksDB) and [tmux](https://linuxize.com/post/getting-started-with-tmux/#installing-tmux) (which runs all nodes and clients in the background). Finally, run a local benchmark using [Fabric](http://www.fabfile.org/):
```
$ fab local
```
This command may take a long time the first time you run it (compiling rust code in `release` mode may be slow), and you can customize a number of benchmark parameters in [fabfile.py](https://github.com/MystenLabs/sui/blob/main/narwhal/benchmark/fabfile.py). When the benchmark terminates, it displays a summary of the execution similarly to the one below.
```
-----------------------------------------
 SUMMARY:
-----------------------------------------
 + CONFIG:
 Faults: 0 node(s)
 Committee size: 4 node(s)
 Worker(s) per node: 1 worker(s)
 Collocate primary and workers: True
 Input rate: 50,000 tx/s
 Transaction size: 512 B
 Execution time: 19 s

 Header size: 1,000 B
 Max header delay: 100 ms
 GC depth: 50 round(s)
 Sync retry delay: 10,000 ms
 Sync retry nodes: 3 node(s)
 batch size: 500,000 B
 Max batch delay: 100 ms

 + RESULTS:
 Consensus TPS: 46,478 tx/s
 Consensus BPS: 23,796,531 B/s
 Consensus latency: 464 ms

 End-to-end TPS: 46,149 tx/s
 End-to-end BPS: 23,628,541 B/s
 End-to-end latency: 557 ms
-----------------------------------------
```

## Next Steps
The next step is to read the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf) and [Bullshark: The Partially Synchronous Version](https://arxiv.org/pdf/2209.05633.pdf). It is then recommended to have a look at the README files of the [worker](worker) and [primary](primary) crates. An additional resource to better understand the Tusk consensus protocol is the paper [All You Need is DAG](https://arxiv.org/abs/2102.08325) as it describes a similar protocol. 

The README file of the [benchmark folder](benchmark) explains how to benchmark the codebase and read benchmarks' results. It also provides a step-by-step tutorial to run benchmarks on [Amazon Web Services (AWS)](https://aws.amazon.com) across multiple data centers (WAN).
