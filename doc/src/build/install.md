---
title: Install Sui
---

Welcome to the Sui development environment! This site is available in two versions in the menu at top left: the default and stable [Devnet](https://docs.sui.io/devnet/learn) branch and the [Latest build](https://docs.sui.io/learn) upstream `main` branch. Use the `devnet` version for app development on top of Sui. Use the Latest build `main` branch for [contributing to the Sui blockchain](../contribute/index.md) itself. Always check and submit fixes to the `main` branch.

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

* [Linux](#linux-specific) - Ubuntu version 18.04 (Bionic Beaver)
* [macOS](#macOS-specific) - macOS Monterey
* [Microsoft Windows](#microsoft-windows-specific) - Windows 11

First install the [General packages](#general-packages) (plus [Brew](#brew) if on macOS), then install the OS-specific packages.

## Prerequisites

At a minimum, you should have a machine capable of installing command line tools (namely, a terminal).
First install the packages outlined this section. Then add the additional dependencies
below for your operating system.

Here are the packages required by operating system:

|Package/OS |Linux  | macOS| Windows 11|
--- | :---: | :---:| :---:|
|Curl|X|X|X|
|Rust|X|X|X|
|Git CLI|X|X|X|
|CMake|X|X|X|
|libssl-dev|X| | |
|libclang-dev|X| | |
|Brew| |X| |
|C++ build tools| | |X|
|LLVM Compiler| | |X|
|Sui|X|X|X|

Follow the instructions below to install them. Then install the Sui [binaries](#binaries).

Finally, if you will be altering Sui itself, also obtain the [Sui source code](#source-code).
For simplicity, we recommend installing in `~/sui` or using an environment variable.

>**Important:** You will need to restart your command prompt after installing these prerequisites
>for them to be available in your environment.

### Brew
In macOS, first install [Brew](https://brew.sh/) to install other packages:
```shell
$ /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### General packages

Ensure each of the packages below exist on each OS:

#### Curl
Confirm that you can run the `curl` command to download dependencies.

See whether you already have curl installed by running:

```shell
$ which curl
```

And if you see no output path, install it with:

*Linux*
```shell
$ apt install curl
```

*macOS*
```shell
$ brew install curl
```

*Microsoft Windows*
Download and install from: https://curl.se/windows/

#### Rust
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

> **Warning:** If you run into issues, you may un-install Rust and Cargo with:
> ```shell
> $ rustup self uninstall
> ```
> And then start the Rust install over.
> For more details, see:
> https://www.rust-lang.org/tools/install

#### Git CLI

Download and install the [`git` command line interface](https://git-scm.com/download/)
for your operating system.

#### CMake

Get the `cmake` command to build Sui:

*Linux*
```shell
$ apt install cmake
```

*macOS*
```shell
$ brew install cmake
```
*Microsoft Windows*

Download and install from: https://cmake.org/download/

If you run into issues, follow this detailed [CMake Installation](https://riptutorial.com/cmake/example/4459/cmake-installation) tutorial.

### Linux-specific

In Linux, install:

libssl-dev
```shell
$ apt install libssl-dev
```

libclang-dev
```shell
$ apt install libclang-dev
```

### macOS-specific

In macOS, other than the aforementioned [Brew](#brew) package manager, the general prerequisites are sufficient.

### Microsoft Windows-specific

In Microsoft Windows 11, also install:

For Windows on ARM64 only - [Visual Studio 2022 Preview](https://visualstudio.microsoft.com/vs/preview/)

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
* [`sui`](cli-client.md) - The Sui CLI tool contains subcommands for enabling `genesis` of validators and accounts, starting the Sui network, and [building and testing Move packages](move/index.md), as well as a [client](cli-client.md) for interacting with the Sui network.
* [`rpc-server`](json-rpc.md) - run a local Sui gateway service accessible via an RPC interface.

### macOS and Linux
Confirm the binaries are installed with:
```
$ echo $PATH
```
### Windows
Confirm the binaries are installed with:
```
$ echo %PATH%
```
And ensure the `.cargo/bin` directory appears. Access the help for any of these binaries by passing the `--help` argument to it.

> **Important:** Make sure your entire toolchain stays up-to-date. If you encounter issues building and installing the Sui binaries, update all packages above and re-install.

## Integrated Development Environment
For Move development, we recommend the [Visual Studio Code (vscode)](https://code.visualstudio.com/) IDE with the Move Analyzer language server plugin installed:

```shell
$ cargo install --git https://github.com/move-language/move move-analyzer --features "address20"
```

Then follow the Visual Studio Marketplace instructions to install the [Move Analyzer extension](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer). (The `cargo install` command for the language server is broken there; hence, we include the correct command above.)

See more [IDE options](https://github.com/MystenLabs/awesome-move#ides) in the [Awesome Move](https://github.com/MystenLabs/awesome-move) docs.

## SUI tokens

To [experiment with Devnet](../build/devnet.md) or [use the Sui Wallet Browser Extension](../explore/wallet-browser.md), you will need SUI tokens. These coins have no financial value and will disappear each time we reset the network.

To request SUI test tokens:

1. Join the [Sui Discord](https://discord.com/invite/sui) If you haven’t already.
1. Identify your address through either the Sui Wallet Browser Extension or by running the following command and electing to connect to a Sui RPC server if prompted:
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
* https://mystenlabs.github.io/narwhal/ - the Narwhal and Bullshark consensus engine
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
