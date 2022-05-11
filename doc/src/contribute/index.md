---
title: Contributing to Sui
---

This page describes how to add to Sui. If you merely need to get the Sui binaries, follow [Install Sui](../build/install.md).

Find answers to common questions in our [FAQ](../contribute/faq.md). Read other sub-pages in this section for ways to contribute to Sui.

## See our roadmap

Sui is evolving quickly. See our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) for the
overall status of Sui, including timelines for launching devnet, testnet, and mainnet.

## Join the community

To connect with the Sui community, join our [Discord](https://discord.gg/sui).

## File issues

Report bugs and make feature requests in the [Sui GitHub](https://github.com/MystenLabs/sui/issues) repository
using the [Template for Reporting Issues](https://github.com/MystenLabs/sui/blob/main/ISSUES.md).

## Provide docs feedback

Send us documentation fixes or requests for improvement at:
doc@mystenlabs.com

You may also suggest changes to the docs directly in GitHub right here using the **Source Code** link below.

Simply edit the file in question and generate a pull request. We will get back to you shortly.

## Download Sui

In order to obtain the Sui source code, follow the steps to download (`git clone`) the `sui` repository
at [Install Sui](../build/install.md#source-code).

And see the Rust [Crates](https://doc.rust-lang.org/rust-by-example/crates.html) in use at:
* https://mystenlabs.github.io/sui/ - the Sui blockchain
* https://mystenlabs.github.io/narwhal/ - the Narwhal and Tusk consensus engine
* https://mystenlabs.github.io/mysten-infra/ - Mysten Labs infrastructure

## Send pull requests

Start by creating your own fork of the repo:
```bash
$ gh repo fork https://github.com/mystenlabs/sui.git # or alternatively, clone your fork
cargo install --path sui/sui # put Sui CLI's in your PATH
cd sui
cargo build --all --all-targets # check that build works
cargo test # check that tests pass
```

To submit your pull request:

1. Make your changes in a descriptively named branch.
2. If you have added code that should be tested, add unit tests.
3. Ensure your code builds and passes the tests: `cargo test`
4. Make sure your code passes the linters and autoformatter: `cargo clippy --all --all-targets && cargo fmt --all`
5. If you have made changes to APIs, update the relevant documentation, and build and test the developer site.
6. Run `git push -f origin <branch_name>`, then open a pull request from the Sui GitHub site.

## Further reading

* Learn [about Mysten Labs](https://mystenlabs.com/) the company on our public site.
* Read the [Sui Smart Contract Platform](../../paper/sui.pdf) white paper.
* Implementing [logging](../contribute/observability.md) in Sui to observe the behavior of your development.
* Find related [research papers](../contribute/research-papers.md).
* See and adhere to our [code of conduct](../contribute/code-of-conduct.md).
