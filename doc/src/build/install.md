---
title: Install Sui
---

Sui is written in Rust, and we are using Cargo to build and manage the
dependencies.  As a prerequisite, you will need to [install
Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)
version 1.59.0 or higher in order to build and install Sui on your machine.

If you'd like to install only Sui binaries (`sui`, `wallet`,
`sui-move`, and `rest_server`), use the following command:

```shell
cargo install --git https://github.com/MystenLabs/sui.git
```

Alternatively, clone the [Sui
GitHub](https://github.com/MystenLabs/sui) repository and then `cargo
install` with the repository clone:

```shell
git clone https://github.com/MystenLabs/sui.git
cargo install --path sui/sui
```

In both cases, this will install `sui`, `wallet`, `sui-move`, and `rest_server`
binaries in a `~/.cargo/bin` directory that can be executed directly.
