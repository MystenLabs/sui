#!/bin/bash
# Prereqs: Rust Cargo, Git CLI, and GitHub account
# Usage: set up environment for Sui development
# Run `sui-setup.sh` in the directory to download source
shopt -s nullglob
set -e
set -o pipefail

## Confirm or get Cargo with Rust toolchain
command -v cargo >/dev/null 2>&1 || { echo "Cargo (https://doc.rust-lang.org/cargo/getting-started/installation.html) is not installed or missing from PATH, exiting."; return 1; }

## Build and install Sui binaries
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "devnet" sui

## Install Move Analyzer language server plugin
cargo install --git https://github.com/move-language/move move-analyzer
## Get the VSCode extension at: https://marketplace.visualstudio.com/items?itemName=move.move-analyzer

## Download Sui source code
git clone https://github.com/MystenLabs/sui.git

## Create Wallet configuration
sui genesis --force

done
# unset it now
shopt -u nullglob
