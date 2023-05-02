---
title: Install Sui to Build
---

Learn how to install and configure Sui.

Before you install Sui, you need to install some prerequisite tools and configure your development environment.

The steps to install Sui include:

1.  Install [prerequisites](#prerequisites) for your operating system.
1.  Install [Sui binaries](#install-sui-binaries).
1.  Configure an [Integrated Development Environment (IDE)](#integrated-development-environment).
1.  Request [SUI test tokens](#sui-tokens) to use on Sui Devnet or Sui Testnet networks.

You can also download the [source code](#source-code) to have local access to files.

## Sui repository

The Sui repository includes primary branches, `devnet`, `testnet`, `mainnet`, and `main`.

- The `devnet` branch includes the latest stable build of Sui. Choose the `devnet` branch if you want to build or test on Sui Devnet. If you encounter an issue or find a bug, it may already be fixed in the `main` branch. To submit a pull request (PR), you should push commits to your fork of the `main` branch.
- The `testnet` branch includes the code running on the Sui Testnet network.
- The `mainnet` branch includes the code running on the Sui Mainnet network.
- The `main` branch includes the most recent changes and updates. Use the `main` branch if you want to contribute to the Sui project. The `main` branch may include unreleased changes, or introduce changes that cause issues in apps created using an earlier version.

## Documentation in the Sui repository

The source for the documentation published on this site also resides in the Sui repository. The site displays the **Mainnet** version of the documentation by default. The content on the site differs between the branches of the repository just like the Sui source code. Use the version of the documentation that corresponds to the Sui network you plan to use. For example, to use the Sui Devnet network, use the **Devnet** version of the documentation. To use the Sui Testnet network, use the **Testnet** version of the documentation.

## Supported operating systems

Sui supports the following operating systems:

- Linux - Ubuntu version 20.04 (Bionic Beaver)
- macOS - macOS Monterey
- Microsoft Windows - Windows 11

## Prerequisites

Install the following prerequisites and tools you need to work with Sui.

| Prerequisite    |         Linux         |        macOS         |             Windows 11             |
| --------------- | :-------------------: | :------------------: | :--------------------------------: |
| cURL            |      [X](#curl)       |     [X](#curl-1)     |            [X](#curl-2)            |
| Rust and Cargo  | [X](#rust-and-cargo)  | [X](#rust-and-cargo) |        [X](#rust-and-cargo)        |
| Git CLI         |     [X](#git-cli)     |   [X](#git-cli-1)    |          [X](#git-cli-2)           |
| CMake           |      [X](#cmake)      |    [X](#cmake-1)     |           [X](#cmake-2)            |
| GCC             |       [X](#gcc)       |                      |                                    |
| libssl-dev      |   [X](#libssl-dev)    |                      |                                    |
| libclang-dev    |  [X](#libclang-dev)   |                      |                                    |
| libpq-dev       |    [X](#libpq-dev)    |                      |                                    |
| build-essential | [X](#build-essential) |                      |                                    |
| Brew            |                       |      [X](#brew)      |                                    |
| C++ build tools |                       |                      | [X](#additional-tools-for-windows) |
| LLVM Compiler   |                       |                      | [X](#additional-tools-for-windows) |

### Rust and Cargo

Sui requires Rust and Cargo (Rust's package manager) on all supported operating systems. The suggested method to install Rust is with `rustup` using cURL.

Some other commands in the installation instructions also require cURL to run. If you can't run the cURL command to install Rust, see the instructions to install cURL for your operating system before you install Rust.

Use the following command to install Rust and Cargo on macOS or Linux:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

If you use Windows 11, see information about using the [Rust installer](https://www.rust-lang.org/tools/install) on the Rust website. The installer checks for C++ build tools and prompts you to install them if necessary. Select the option that best defines your environment and follow the instructions in the install wizard.

For additional installation options, see [Install Rust](https://www.rust-lang.org/tools/install).

Sui uses the latest version of Cargo to build and manage dependencies. See the [Cargo installation](https://doc.rust-lang.org/cargo/getting-started/installation.html) page on the Rust website for more information.

Use the following command to update Rust with `rustup`:

```shell
rustup update stable
```

After you install Rust, proceed to the prerequisites for your operating system.

- [Linux prerequisites](#linux-prerequisites)
- [macOS prerequisites](#macos-prerequisites)
- [Windows prerequisites](#windows-prerequisites)

## Linux prerequisites

**Note:** The Linux instructions assume a distribution that uses the APT package manager. You might need to adjust the instructions to use other package managers.

Install the prerequisites listed in this section. Use the following command to update `apt-get`:

```shell
sudo apt-get update
```

### cURL

Install cURL with the following command:

```shell
sudo apt install curl
```

Verify that cURL installed correctly with the following command:

```shell
curl --version
```

### Git CLI

Run the following command to install Git, including the [Git CLI](https://cli.github.com/):

```shell
sudo apt-get install git-all
```

For more information, see [Install Git on Linux](https://github.com/git-guides/install-git#install-git-on-linux) on the GitHub website.

### CMake

Use the following command to install CMake.

```shell
sudo apt-get install cmake
```

To customize the installation, see [Installing CMake](https://cmake.org/install/) on the CMake website.

### GCC

Use the following command to install the GNU Compiler Collection, `gcc`:

```shell
sudo apt-get install gcc
```

### libssl-dev

Use the following command to install `libssl-dev`:

```shell
sudo apt-get install libssl-dev
```

If the version of Linux you use doesn't support `libssl-dev`, find an equivalent package for it on the [ROS Index](https://index.ros.org/d/libssl-dev/).

(Optional) If you have OpenSSL you might also need to also install `pkg-config`:

```shell
sudo apt-get install pkg-config
```

### libclang-dev

Use the following command to install `libclang-dev`:

```shell
sudo apt-get install libclang-dev
```

If the version of Linux you use doesn't support `libclang-dev`, find an equivalent package for it on the [ROS Index](https://index.ros.org/d/libclang-dev/).

### libpq-dev

Use the following command to install `libpq-dev`:

```shell
sudo apt-get install libpq-dev
```

If the version of Linux you use doesn't support `libpq-dev`, find an equivalent package for it on the [ROS Index](https://index.ros.org/d/libpq-dev/).

### build-essential

Use the following command to install `build-essential`"

```shell
sudo apt-get install build-essential
```

Proceed to [Install Sui binaries](#install-sui-binaries) to continue installing Sui.

## macOS prerequisites

macOS includes a version of cURL you can use to install Brew. Use Brew to install other tools, including a newer version of cURL.

### Brew

Use the following command to install [Brew](https://brew.sh/):

```shell
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### cURL

Use the following command to update the default [cURL](https://curl.se) on macOS:

```shell
brew install curl
```

### CMake

Use the following command to install CMake:

```shell
brew install cmake
```

To customize the installation, see [Installing CMake](https://cmake.org/install/) on the CMake website.

### Git CLI

Use the following command to install Git:

```shell
brew install git
```

After installing Git, download and install the [Git command line interface](https://git-scm.com/download/).

Proceed to [Install Sui binaries](#install-sui-binaries) to continue installing Sui.

## Windows prerequisites

Install the following prerequisites to work with Sui on Microsoft Windows 11.

### cURL

Windows 11 ships with a Microsoft version of [cURL](https://curl.se/windows/microsoft.html) already installed. If you want to use the curl project version instead, download and install it from [https://curl.se/windows/](https://curl.se/windows/).

### Git CLI

Download and install the [Git command line interface](https://git-scm.com/download/).

### CMake

Download and install [CMake](https://cmake.org/download/) from the CMake website.

### Protocol Buffers

Download [Protocol Buffers](https://github.com/protocolbuffers/protobuf/releases) (protoc-xx.x-win32.zip or protoc-xx.x-win64.zip) and add the \bin directory to your Windows PATH environment variable.

### Additional tools for Windows

Sui requires the following additional tools on computers running Windows.

- For Windows on ARM64 only - [Visual Studio 2022 Preview](https://visualstudio.microsoft.com/vs/preview/).
- [C++ build tools](https://visualstudio.microsoft.com/downloads/) is required to [install Rust](#rust-and-cargo).
- The [LLVM Compiler Infrastructure](https://releases.llvm.org/). Look for a file with a name similar to LLVM-15.0.7-win64.exe for 64-bit Windows, or LLVM-15.0.7-win32.exe for 32-bit Windows.

**Known issue** - The `sui console` command does not work in PowerShell.

## Install Sui binaries

Run the following command to install Sui binaries from the `devnet` branch:

```shell
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch devnet sui
```

The install process can take a while to complete. You can monitor installation progress in the terminal. If you encounter an error, make sure to install the latest version of all prerequisites and then try the command again.

To update to the latest stable version of Rust:

```shell
rustup update stable
```

The command installs Sui components in the `~/.cargo/bin` folder.

### Confirm the installation

To confirm that Sui installed correctly, type `sui` and press Enter. You should see a message about the Sui version installed and help for using Sui commands.

## Integrated development environment

The recommended IDE for Move development is [Visual Studio Code](https://code.visualstudio.com/) with the move-analyzer extension. Follow the Visual Studio Marketplace instructions to install the [move-analyzer extension](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer), then install the move-analyzer language server passing `address32` using the `--features` flag and passing `sui-move` to the `branch` flag:

```shell
cargo install --git https://github.com/move-language/move move-analyzer --branch sui-move --features "address32"
```

See more [IDE options](https://github.com/MystenLabs/awesome-move#ides) in the [Awesome Move](https://github.com/MystenLabs/awesome-move) documentation.

## SUI tokens

You need SUI tokens to perform transactions on a Sui network. You can get test tokens from the Sui faucet in Discord, or directly in the [Sui Wallet](https://github.com/MystenLabs/mysten-app-docs/blob/main/mysten-sui-wallet.md).

To request SUI test tokens in Discord:

1.  Join the [Sui Discord](https://discord.com/invite/sui) If you haven’t already.
1.  Identify your address through either the Sui Wallet browser extension or by running the following command and electing to connect to a Sui RPC server if prompted:

```shell
sui client active-address
```

1.  Request tokens in the [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) channel using the syntax: `!faucet <YOUR_ADDRESS>`, for example:
    `shell
      !faucet 0xa56612ad4f5dbc04c651e8d20f56af3316ee6793335707f29857bacabf9127d0
      `
    A bot on the channel distributes tokens to your address.

## Source code

View the Sui repository on GitHub:
https://github.com/MystenLabs/sui

Clone the Sui repository:

```shell
git clone https://github.com/MystenLabs/sui.git --branch devnet
```

The following primary directories offer a good starting point for exploring Sui's source code:

- [sui](https://github.com/MystenLabs/sui/tree/main/crates/sui) - Sui, including the Sui CLI Client
- [sui_programmability](https://github.com/MystenLabs/sui/tree/main/sui_programmability) - Sui Move code examples (games, defi, nfts, ...)
- [sui_core](https://github.com/MystenLabs/sui/tree/main/crates/sui-core) - Core Sui components
- [sui-types](https://github.com/MystenLabs/sui/tree/main/crates/sui-types) - Sui object types, such as coins and gas
- [explorer](https://github.com/MystenLabs/sui/tree/main/apps/explorer) - browser-based object explorer for the Sui network
- [sui-network](https://github.com/MystenLabs/sui/tree/main/crates/sui-network) - networking interfaces
