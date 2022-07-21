---
title: Install Sui
---

Welcome to the Sui development environment! Note, this site is built from the upstream `main`
branch and therefore will contain updates not yet found in `devnet`. The instructions here
recommend use of `devnet` as the latest stable release. To [contribute to Sui](../contribute/index.md),
instead use the `main` branch.

## Summary

To immediately get started using Sui:

1. Meet the [prerequisites](#prerequisites).
1. Install the [binaries](#binaries).
1. Configure an [Integrated Development Environment (IDE)](#integrated-development-environment).
1. Request [SUI tokens](#sui-tokens) to evaluate Devnet and Sui Wallet
1. Optionally, download the [source code](#source-code) to have local
   access to examples and modify Sui itself.

> **Tip:** Assuming you on macOS or Linux, have `curl`, Rust Cargo, the `git` command, and a GitHub account
> (see [Prerequisites](#prerequisites)), you can download the `sui-setup.sh` script
> and run it to conduct all of the setup below, **including removal of any existing
> sui assets**. To use it, run these commands in a terminal:
> ```shell
> $ curl https://raw.githubusercontent.com/MystenLabs/sui/main/doc/utils/sui-setup.sh -o sui-setup.sh
> chmod 755 sui-setup.sh
> ./sui-setup.sh
> ```

## Supported OSes

The following operating systems (OSes) have been tested and are supported for
running Sui:

* Linux - Ubuntu version 18.04 (Bionic Beaver)
* macOS - macOS Monterey
* Microsoft Windows - Windows 11

## Prerequisites

At a minimum, you should have a machine capable of installing command line tools (namely, a terminal).
First install the packages outlined this section. Then add the additional dependencies
below for your operating system.

Finally, if you will be altering Sui itself, also obtain the [Sui source code](#source-code).
For simplicity, we recommend installing in `~/sui` or using an environment variable.

>**Important:** You will need to restart your command prompt after installing these prerequisites
>for them to be available in your environment.

### Sui binaries
Install the Sui [binaries](#binaries) as described below.

### Curl
Confirm that you can run the `curl` command to download dependencies.

See whether you already have curl installed by running:

```shell
$ which curl
```

And if you see no output path, install it with:

```shell
$ sudo apt install curl
```

### Rust
Sui is written in Rust, and we are using the latest version of the
[Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) toolchain
to build and manage the dependencies. You will need Cargo to build and install Sui on your machine.

Get [rustup](https://rust-lang.github.io/rustup/)
to install Rust and Cargo:

```shell
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then update the packages with:

```shell
$ rustup update stable
```

If you run into issues, re-install Rust and Cargo:

```shell
$ sudo apt remove cargo
sudo apt autoremove
```

And then start the Rust install over.
For more details, see:
https://www.rust-lang.org/tools/install

### Git CLI

Download and install the [`git` command line interface](https://git-scm.com/download/)
for your operating system.

### CMake

Get the `cmake` command to build Sui:

```shell
$ sudo apt install cmake
```

If you run into issues, follow this detailed [CMake Installation](https://riptutorial.com/cmake/example/4459/cmake-installation) tutorial.

### Linux

In Linux, also install:

libssl-dev
```shell
$ sudo apt install libssl-dev
```

libclang-dev
```shell
$ sudo apt install libclang-dev
```

### macOS

In macOS, the general prerequisites outlined above are sufficient.

### Microsoft Windows

In Microsoft Windows, also install:

[C++ build tools](https://visualstudio.microsoft.com/downloads/)

The [LLVM Compiler Infrastructure](https://releases.llvm.org/)

>**Tip:** The installation progress might appear hanging if the `cmd.exe` window loses focus;
>press the `enter` key in the command prompt fix the issue.

>**Known Issue:** The `sui console` command does not work in PowerShell.

## Binaries

To develop in Sui, you will need the Sui binaries. After installing `cargo`, run:

```shell
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "devnet" sui sui-gateway
```

This will put the following binaries in your `PATH` (ex. under `~/.cargo/bin`) that provide these command line interfaces (CLIs):
* sui - The Sui CLI tool contains subcommands for enabling `genesis` of validators and accounts, starting the Sui network, and [building and testing Move packages](move/index.md), as well as a [client](cli-client.md) for interacting with the Sui network.
* [`rpc-server`](json-rpc.md) - run a local Sui gateway service accessible via an RPC interface.

Confirm the installation with:
#### macOS and Linux
```
$ echo $PATH
```
#### Windows
```
$ echo %PATH%
```
And ensure the `.cargo/bin` directory appears. Access the help for any of these binaries by passing the `--help` argument to it.

## Integrated Development Environment
For Move development, we recommend the [Visual Studio Code (vscode)](https://code.visualstudio.com/) IDE with the Move Analyzer language server plugin installed:

```shell
$ cargo install --git https://github.com/move-language/move move-analyzer --features "address20"
```

Then follow the Visual Studio Marketplace instructions to install the [Move Analyzer extension](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer). (The `cargo install` command for the language server is broken there; hence, we include the correct command above.)

See more [IDE options](https://github.com/MystenLabs/awesome-move#ides) in the [Awesome Move](https://github.com/MystenLabs/awesome-move) docs.

## SUI tokens

To [experiment with Devnet](../explore/devnet.md) or [use the Sui Wallet Browser Extension](../explore/wallet-browser.md), you will need SUI tokens. These coins have no financial value and will disappear each time we reset the network.

To request SUI test tokens:

1. Join the [Sui Discord](https://discord.com/invite/sui) If you haven’t already.
1. Identify your address through either the Sui Wallet Browser Extension or by running the command:
   ```shell
   $ sui client active-address
   ```
1. Request tokens in the [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) channel using the syntax: `!faucet <YOUR_ADDRESS>`, for example:
      ```shell
      !faucet 0xd72c2c90ed9d923cb0ed2ca91db5be9e1c9b5ccb
      ```
1. A bot on the channel will distribute tokens to you automatically.

## Source code

If you need to download and understand the Sui source code:
https://github.com/MystenLabs/sui

Clone the Sui repository:

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

## Rustdoc

See the Rust [Crates](https://doc.rust-lang.org/rust-by-example/crates.html) in use at:
* https://mystenlabs.github.io/sui/ - the Sui blockchain
* https://mystenlabs.github.io/narwhal/ - the Narwhal and Tusk consensus engine
* https://mystenlabs.github.io/mysten-infra/ - Mysten Labs infrastructure

## Help

To contribute updates to Sui code, [send pull requests](../contribute/index.md#send-pull-requests) our way.

> NOTE: the above `git clone` command syncs with the `devnet` branch, which makes sure the source code is compatible with our Devnet. If you want to run network locally using the latest version and don't need to interact with our Devnet, you should switch to `main` branch.
 
## Next steps

Continue your journey through:

* [Smart Contracts with Move](move/index.md)
* [Sui client Quick Start](cli-client.md)
* [RPC Server API](json-rpc.md)
* [End-to-End tutorial](../explore/tutorials.md)
