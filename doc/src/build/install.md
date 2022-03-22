---
title: Install Sui
---

Sui is written in Rust, and we are using Cargo to build and manage the
dependencies.  As a prerequisite, you will need to [install
Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)
version 1.59.0 or higher in order to build and install Sui on your machine.

## Set up

Although you may create whatever directory structure you desire, both our
[Wallet Quick Start](wallet.md) and [end-to-end tutorial](../explore/tutorials.md)
assume Sui is installed in a directory found by a `$SUI_ROOT` environment variable.

To set this up, run the following commands and substitute in your path and
desired directory name:

```shell
mkdir some-dir
export SUI_ROOT=/path/to/some-dir
```

## Download

Navigate to your desired install location, for example:

```shell
cd "$SUI_ROOT"
```

### Binaries only

If you'd like to install only Sui binaries (`sui`, `wallet`,
`sui-move`, and `rest_server`), use the following command:

```shell
cargo install --git https://github.com/MystenLabs/sui.git
```

### Whole repository

Alternatively, clone the [Sui
GitHub](https://github.com/MystenLabs/sui) repository and then `cargo
install` with the repository clone:

```shell
git clone https://github.com/MystenLabs/sui.git
cargo install --path sui/sui
```

## Use

Either method will install `sui`, `wallet`, `sui-move`, and `rest_server`
binaries in a `~/.cargo/bin` directory that can be executed directly.
