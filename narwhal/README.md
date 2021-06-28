# Narwhal and Tusk

[![build status](https://img.shields.io/github/workflow/status/facebookresearch/narwhal/Rust/master?style=flat-square&logo=github)](https://github.com/facebookresearch/narwhal/actions)
[![rustc](https://img.shields.io/badge/rustc-1.52+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

This repo contains a prototype of Narwhal and Tusk. It supplements the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf).

## Overview
We propose separating the task of transaction dissemination from transaction ordering, to enable high-performance Byzantine fault-tolerant consensus in a permissioned setting. To this end, we design and evaluate a mempool protocol, Narwhal, specializing in high-throughput reliable dissemination and storage of causal histories of transactions. We also propose Tusk, a zero-message overhead asynchronous consensus protocol embedded within Narwhal. Further, Narwhal is designed to easily scale-out using multiple workers at each validator.

## Getting Started
The core protocols are written in Rust, but all benchmarking scripts are written in Python and run with [Fabric](http://www.fabfile.org/).
To deploy and benchmark a testbed of 4 nodes on your local machine, clone the repo and install the python dependencies:
```
$ git clone https://github.com/facebookresearch/narwhal.git
$ cd narwhal/benchmark
$ pip install -r requirements.txt
```
You also need to install Clang (required by rocksdb) and [tmux](https://linuxize.com/post/getting-started-with-tmux/#installing-tmux) (which runs all nodes and clients in the background). Finally, run a local benchmark using fabric:
```
$ fab local
```
This command may take a long time the first time you run it (compiling rust code in `release` mode may be slow) and you can customize a number of benchmark parameters in `fabfile.py`. When the benchmark terminates, it displays a summary of the execution similarly to the one below.
```
-----------------------------------------
 SUMMARY:
-----------------------------------------
 + CONFIG:
 Committee size: 4 node(s)
 Input rate: 50,000 tx/s
 Transaction size: 512 B
 Faults: 0 node(s)
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

## License
This software is licensed as [Apache 2.0](LICENSE).
