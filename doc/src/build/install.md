---
title: Install Sui
---

Sui is written in Rust, and we are using Cargo to build and manage the
dependencies.  As a prerequisite, you will need to [install
Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)
version 1.59.0 or higher in order to build and install Sui on your machine.

### CLIs

After installing `cargo`, run:

```shell
cargo install --git https://github.com/MystenLabs/sui.git
```

This will put three binaries in your `PATH`:
* [`sui-move`](move.md): Build and test Move packages.
* [`wallet`](wallet.md): Run a local Sui network and gateway service accessible via the wallet CLI. The wallet CLI manage keypairs to sign/send transactions.
* [`rest_server`](rest-api.md): Run a local Sui network and gateway service accessible via a REST interface.

### IDE
For Move development, the best IDE is vscode with [this](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer) plugin. Some alternative IDE options are [here]( https://github.com/MystenLabs/awesome-move#ide).

## Next steps

Continue your journey through:

* [Smart Contracts with Move](move.md)
* [Wallet Quick Start](wallet.md)
* [REST Server API](rest-api.md)
* [End-to-End tutorial](../explore/tutorials.md)
