---
title: Install Sui
---

Sui is written in Rust, and we are using Cargo to build and manage the
dependencies.  As a prerequisite, you will need to [install
cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)
in order to build and install Sui on your machine (you will need cargo
1.59.0 or higher).

If you'd like to only install Sui binaries (`sui`, `wallet`, and
`sui-move`), use the following command:

```shell
cargo install --git ssh://git@github.com/MystenLabs/sui.git
```

Alternatively, clone the Sui [Sui
GitHub](https://github.com/MystenLabs/sui) repository and the `cargo
install` with the repository clone:

```shell
git clone https://github.com/MystenLabs/sui.git
cargo install --path sui
```

In both cases, this will install `sui`, `wallet`, and `sui-move`
binaries in `~/.cargo/bin`directory that can be executed directly.
