# Tusk

[![rustc](https://img.shields.io/badge/rustc-1.48+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

The code in this branch is a prototype of Tusk. It suplements the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf) enabling reproducible resuts. All data points used to produce the graphs of the paper are available in the folder [results/data](/results/data). There are no plans to maintain this branch. The [master branch](https://github.com/facebookresearch/narwhal) contains the most recent and polished version of this codebase. 


**Note:** Please run tests on a single thread:
```
cargo test -- --test-threads 1
```

## License
This software is licensed as [Apache 2.0](LICENSE).
