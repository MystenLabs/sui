# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
# Prereqs: Rust Cargo, Git CLI, and GitHub account
# Usage: set up environment for Sui development
# First run these commands tow download the script
# curl https://raw.githubusercontent.com/MystenLabs/sui/main/doc/utils/sui-setup.sh -o sui-setup.sh
# chmod 755 sui-setup.sh
# Then run `sui-setup.sh` in a directory to download source
# For example, to use the same
# ./sui-setup.sh
shopt -s nullglob
set -e
set -o pipefail

## Confirm or get Cargo with Rust toolchain
command -v cargo >/dev/null 2>&1 || { echo "Cargo (https://doc.rust-lang.org/cargo/getting-started/installation.html) is not installed or missing from PATH, exiting."; return 1; }

## Remove sui directory if it already exists
rm -rf sui/

## Build and install Sui binaries
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "devnet" sui

## Install Move Analyzer language server plugin
cargo install --git https://github.com/move-language/move move-analyzer

## Download Sui source code
git clone https://github.com/MystenLabs/sui.git

## Create Wallet configuration
sui genesis --force

## Recommend manual install of VSCode extension
echo "Now install the Move Analyzer VSCode extension per: https://marketplace.visualstudio.com/items?itemName=move.move-analyzer"

# unset it now
shopt -u nullglob
