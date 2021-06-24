# Narwhal and Tusk

[![rustc](https://img.shields.io/badge/rustc-1.48+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

This repo contains a prototype of Narwhal and Tusk. It supplements the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf).

## Overview
We propose separating the task of transaction dissemination from transaction ordering, to enable high-performance
Byzantine fault-tolerant consensus in a permissioned setting. To this end, we design and evaluate a mempool protocol,
Narwhal, specializing in high-throughput reliable dissemination and storage of causal histories of transactions. Narwhal
tolerates an asynchronous network and maintains its performance despite failures. We demonstrate that composing
Narwhal with a partially synchronous consensus protocol (HotStuff) yields significantly better throughput even in the
presence of faults. However, loss of liveness during view-changes can result in high latency. To achieve overall good
performance when faults occur we propose Tusk, a zero-message overhead asynchronous consensus protocol embedded within Narwhal. We demonstrate its high performance under a variety of configurations and faults. Further, Narwhal is designed to easily scale-out using multiple workers at each validator, and we demonstrate that there is no foreseeable limit to the throughput we can achieve for consensus,
with a few seconds latency.

As a summary of results, on a Wide Area Network (WAN), Hotstuff over Narwhal achieves 170,000 tx/sec with a 2.5-sec
latency instead of 1,800 tx/sec with 1-sec latency of Hotstuff. 
Additional workers increase throughput linearly to 600,000
tx/sec without any latency increase. Tusk achieves 140,000
tx/sec with 4 seconds latency or 20x better than the state-of-the-art asynchronous protocol. Under faults, both Narwhal
based protocols maintain high throughput, but the HotStuff
variant suffers from slightly higher latency.

## Getting Started
The core protocols are written in Rust, but all benchmarking scripts are written in Python and run with [Fabric](http://www.fabfile.org/).
To deploy and benchmark a testbed of 4 nodes on your local machine, clone the repo and compile it in release mode:
```
$ git clone https://github.com/facebookresearch/narwhal.git
$ cd rust
$ cargo build --release
```
Then install the Python dependencies:
```
$ cd ../scripts
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
 Committee size: 4 nodes
 Number of workers: 1 worker(s) per node
 Faults: 0 nodes
 Transaction size: 512 B
 Max batch size: 1,000 txs
 Transaction rate: 60,000 tx/s

 Dag Results:
 + Total certified bytes: 799,468,544 B
 + Execution time: 29,646 ms
 + Estimated BPS: 26,967,619 B/s
 + Estimated TPS: 52,671 txs/s
 + Block Latency: 6 ms
 + Client Latency: 93 ms

 Consensus Results:
 + Total committed bytes: 786,986,496 B
 + Execution time: 29,542 ms
 + Estimated BPS: 26,639,130 B/s
 + Estimated TPS: 52,030 txs/s
 + Block Latency: 395 ms
 + Client Latency: 482 ms
-----------------------------------------
```

## License
This software is licensed as [Apache 2.0](LICENSE).