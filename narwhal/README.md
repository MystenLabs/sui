# Tusk

[![rustc](https://img.shields.io/badge/rustc-1.48+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

The code in this branch is a prototype of Tusk. It suplements the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf) and is used to produce the some of the paper's figures.

Please run tests on a single thread:
```
cargo test -- --test-threads 1
```

## License
This software is licensed as [Apache 2.0](LICENSE).
