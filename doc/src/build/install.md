---
title: Install Sui
---

Welcome to the Sui development environment! Note, this site is built from the upstream `main`
branch and therefore will contain updates not yet found in `devnet`. The instructions here
recommend use of `devnet` as the latest stable release. To [contribute to Sui](../contribute/index.md),
instead use the `main` branch.

To immediately get started using Sui:

1. Meet the [prerequisites](#prerequisites).
2. Install the [binaries](#binaries).
3. Configure an [Integrated Development Environment (IDE)](#integrated-development-environment).
4. Optionally, download the [source code](#source-code) to have local
   access to examples and modify Sui itself.

> **Tip:** Assuming you have Rust Cargo, the `git` command, and a GitHub account
> (see prerequisites(#prerequisites)), you can download the `sui-setup.sh` script
> and run it to conduct all of the setup below, **including removal of any existing
> sui assets**. To use it, run these commands in a terminal:
> ```shell
> $ curl https://raw.githubusercontent.com/MystenLabs/sui/main/doc/utils/sui-setup.sh -o sui-setup.sh
> chmod 755 sui-setup.sh
> ./sui-setup.sh
> ```

## Prerequisites

At a minimum, you should have a machine capable of installing command line tools.
These prerequisites are broken down into the [essential](#essential) tools
you need to work in Sui and the [advanced](#advanced) items needed for Sui source
code development.

### Essential

Sui is written in Rust, and we are using Cargo to build and manage the
dependencies. You will need Cargo to build and install Sui on your machine.

To run Sui, you will need to install:
1. A command line interface, as virtually everything done here is done by CLI.
1. The `curl` command to download other tools, which you can confirm with:
   ```shell
   $ which curl
1. The [Rust and Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) toolchain version 1.60.0 or higher; update it with:
   ```shell
   $ rustup update stable
   ```
1. The `cmake` command.
1. The Sui [binaries](#binaries).

### Advanced

In addition, to conduct advanced work such as altering Sui itself, also obtain:
1. The [`git` command line interface](https://git-scm.com/download/).
1. The [Sui source code](#source-code); for simplicity, we recommend installing in `~/sui` or using an environment variable

## Binaries

To develop in Sui, you will need the Sui binaries. After installing `cargo`, run:

```shell
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "devnet" sui sui-json-rpc
```

This will put the following binaries in your `PATH` (ex. under `~/.cargo/bin`) that provide these command line interfaces (CLIs):
* sui - The Sui CLI tool contains subcommands for enabling `genesis` of validators and accounts, starting the Sui network, and [building and testing Move packages](move.md), as well as a [client](cli-client.md) for interacting with the Sui network.
* [`rpc-server`](json-rpc.md) - run a local Sui gateway service accessible via an RPC interface.

Confirm the installation with:

```
$ echo $PATH
```

And ensure the `.cargo/bin` directory appears. Access the help for any of these binaries by passing the `--help` argument to it.

## Integrated Development Environment
For Move development, we recommend the [Visual Studio Code (vscode)](https://code.visualstudio.com/) IDE with the Move Analyzer language server plugin installed:

```shell
$ cargo install --git https://github.com/move-language/move move-analyzer
```

Then follow the Visual Studio Marketplace instructions to install the [Move Analyzer extension](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer). (The `cargo install` command for the language server is broken there; hence, we include the correct command above.)

See more [IDE options](https://github.com/MystenLabs/awesome-move#ides) in the [Awesome Move](https://github.com/MystenLabs/awesome-move) docs.

## Source code

If you need to download and understand the Sui source code, clone the Sui repository:

```shell
$ git clone https://github.com/MystenLabs/sui.git --branch devnet
```

You can start exploring Sui's source code by looking into the following primary directories:
* [sui](https://github.com/MystenLabs/sui/tree/main/crates/sui) - the Sui CLI binary
* [sui_programmability](https://github.com/MystenLabs/sui/tree/main/sui_programmability) - Sui's Move language integration also including games and other Move code examples for testing and reuse
* [sui_core](https://github.com/MystenLabs/sui/tree/main/crates/sui-core) - authority server and Sui Gateway
* [sui-types](https://github.com/MystenLabs/sui/tree/main/crates/sui-types) - coins, gas, and other object types
* [explorer](https://github.com/MystenLabs/sui/tree/main/explorer) - object explorer for the Sui network
* [sui-network](https://github.com/MystenLabs/sui/tree/main/crates/sui-network) - networking interfaces

And see the Rust [Crates](https://doc.rust-lang.org/rust-by-example/crates.html) in use at:
* https://mystenlabs.github.io/sui/ - the Sui blockchain
* https://mystenlabs.github.io/narwhal/ - the Narwhal and Tusk consensus engine
* https://mystenlabs.github.io/mysten-infra/ - Mysten Labs infrastructure

To contribute updates to Sui code, [send pull requests](../contribute/index.md#send-pull-requests) our way.

> NOTE: the above `git clone` command syncs with the `devnet` branch, which makes sure the source code is compatible with our devnet. If you want to run network locally using the latest version and don't need to interact with our devnet, you could switch to `main` branch.
## Next steps

Continue your journey through:

* [Smart Contracts with Move](move.md)
* [Sui client Quick Start](cli-client.md)
* [RPC Server API](json-rpc.md)
* [End-to-End tutorial](../explore/tutorials.md)
