---
title: Install Sui to Build
---

Learn how to install and configure Sui.

Before you install Sui, you need to install some prerequisite tools and then configure your environment.

The steps to install Sui include:

1. Install [prerequisites](#prerequisites).
1. Install Sui [binaries](#binaries).
1. Configure an [Integrated Development Environment (IDE)](#integrated-development-environment).
1. Request [SUI tokens](#sui-tokens) to evaluate Devnet and Sui Wallet
1. Optionally, download the [source code](#source-code) to have local
   access to examples and modify Sui itself.

## Branches of the Sui repo

The Sui repo includes two primary branches, `devnet` and `main`.

 * The `devnet` branch includes the latest stable build of Sui. Choose the `devnet` branch if you want to build or test on Sui. If you encounter an issue or find a bug, it may already be fixed in the `main` branch. To submit a Pull Request (PR), you should push commits to your fork of the `main` branch.
 * The `main` (Latest build) branch includes the most recent changes and updates. Use the `main` (Latest build) branch if you want to contribute to the Sui project. The `main` branch may include unreleased changes, or introduce changes that cause issues in apps created using an earlier version.

## Choose the documentation branch

This documentation is built from the same branches, `main` and `devnet`. The `main` branch includes the latest additions and updates to the documentation. You can view the content to learn about upcoming updates to the documentation, but the information may not be accurate or up-to-date for the features and functionality available in the `devnet` branch. In most cases, you should view the `devnet` version of the documentation.

To change branches, choose **Latest build** to view the documentation generated from the `main` branch of the repository. You should not use the **Latest build** version to learn how to install, configure, or build on Sui, as the information may change before the content is merged to **Devnet**. 

## Supported Operating Systems

Sui supports the following operating systems.

* Linux - Ubuntu version 18.04 (Bionic Beaver)
* macOS - macOS Monterey
* Microsoft Windows - Windows 11

## Prerequisites

Install the prerequisites and tools you need to work with Sui. 

| Package/OS | Linux  | macOS | Windows 11 |
| --- | :---: | :---: | :---: |
| Curl | X | X | X |
| Rust | X | X | X |
| Git CLI | X | X | X |
| CMake | X | X | X |
| libssl-dev | X | | |
| libclang-dev | X | | |
| Brew | | X | |
| C++ build tools | | | X |
| LLVM Compiler | | | X |


### Rust and Cargo

Sui requires Rust and Cargo on all supported operating systems. 

Use the following command to install Rust:
`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

For additional installation options, see [Install Rust](https://www.rust-lang.org/tools/install).

The `rustup` script also installs Cargo.

Sui uses the latest version of Cargo to build and manage dependencies. See the [Cargo installation](https://doc.rust-lang.org/cargo/getting-started/installation.html) page for more information.

Use the following command to update Rust:

```shell
$ rustup update stable
```

After you install Rust, proceed to the prerequisites for your operating system.
 * [Linux prerequisites](#linux-prerequisites)
 * [macOS prerequisites](#macos-prerequisites)
 * [Windows prerequisites](#windows-prerequisites)

## Linux prerequisites 

Install the prerequisites listed in this section. You should make sure that your system has the latest version of `apt`. Use the following command to update it:
`sudo apt-get update`

### cURL

Install cURL with the following command:
`sudo apt install curl`

Verify that cURL installed correctly with the following command:
`curl --version`

### Git CLI

Run the following command to install Git, including the Git CLI:

`sudo apt-get install git-all`

For more information, see [Install Git on Linux](https://github.com/git-guides/install-git#install-git-on-linux)

### CMake

Install CMake with the following commands:

`./bootstrap`
`make`
`make install`

For more information, see [Install CMake](https://cmake.org/install/)

### libssl-dev

use the following command to install `libssl-dev`:
`sudo apt-get install libssl-dev`

### libclang-dev

use the following command to install `libclang-dev`:
`sudo apt-get install libclang-dev`

Proceed to [Install Sui binaries](#binaries) to continue installing Sui.


## macOS prerequisites

macOS includes a version of cURL. Use cURL to install Brew, and then use Brew to install other tools, including a newer version of cURL.

### Brew

Use the following command to install [Brew](https://brew.sh/):
```shell
$ /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### cURL

Use the following command to install [cURL](https://curl.se):
`brew install curl`

### CMake

Use the following command to install CMake:
`brew install cmake`

### Git CLI

Use the following command to install Git:
`brew install git`

You can also Download and install the [Git command line interface](https://git-scm.com/download/) for your operating system.

Proceed to [Install Sui binaries](#install-sui-binaries) to continue installing Sui.


## Windows prerequisites

Install the prerequisites listed in the following section if to work with Sui on  Microsoft Windows.

### cURL

Download and install [cURL](https://curl.se) from https://curl.se/windows/.

### Git CLI

Download and install the [Git command line interface](https://git-scm.com/download/)
for your operating system.

### CMake

Download and install [CMake](https://cmake.org/) from: https://cmake.org/download/

### Protocol Buffers

Download [Protocol Buffers](https://github.com/protocolbuffers/protobuf/releases) (protoc-xx.x-win32.zip or protoc-xx.x-win64.zip) and add the \bin directory to your Windows PATH environment variable.

### Additional tools for Windows

Sui requires the following additional tools on computers running Windows.

 * For Windows on ARM64 only - [Visual Studio 2022 Preview](https://visualstudio.microsoft.com/vs/preview/)
 * [C++ build tools](https://visualstudio.microsoft.com/downloads/)
 * The [LLVM Compiler Infrastructure](https://releases.llvm.org/)

>**Tip:** The installation progress might appear hanging if the `cmd.exe` window loses focus;
>press the `enter` key in the command prompt fix the issue.

>**Known Issue:** The `sui console` command does not work in PowerShell.


## Install Sui binaries

After you install Cargo, use the following command to install Sui binaries:

```shell
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch devnet sui sui-node
```

The command installs the following Sui components in `~/.cargo/bin`:
* [`sui`](cli-client.md) - The Sui CLI tool contains subcommands for enabling `genesis` of validators and accounts, starting the Sui network, and [building and testing Move packages](move/index.md), as well as a [client](cli-client.md) for interacting with the Sui network.
* `Sui node` - installs the Sui node binary.

Trouble shooting:
If the previous command fails, make sure you have the latest version of Rust installed:

```
rustup update stable
source "$HOME/.cargo/env"
```

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
For Move development, we recommend the [Visual Studio Code](https://code.visualstudio.com/) IDE with the Move Analyzer language server plugin installed:

```shell
$ cargo install --git https://github.com/move-language/move move-analyzer --features "address20"
```

Then follow the Visual Studio Marketplace instructions to install the [Move Analyzer extension](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer). (The `cargo install` command for the language server is broken there; hence, we include the correct command above.)

See more [IDE options](https://github.com/MystenLabs/awesome-move#ides) in the [Awesome Move](https://github.com/MystenLabs/awesome-move) docs.

## SUI tokens

To [experiment with Devnet](../build/devnet.md) or [use the Sui Wallet Browser Extension](../explore/wallet-browser.md), you can add SUI tokens to your account address. 

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
1. A bot on the channel distributes tokens to you automatically.

## Source code

View the Sui repo on GitHub:
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
* [explorer](https://github.com/MystenLabs/sui/tree/main/apps/explorer) - object explorer for the Sui network
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
