#!/bin/bash

# Create empty directories.
mkdir -p data/faucet.wal
mkdir -p data/logs
mkdir -p data/config
mkdir -p data/genesis
mkdir -p data/txs

# Build binaries from this code base.

# DON'T BUILD sui AND sui-faucet FROM SOURCE
# BECAUSE THEY CURRENTLY DO NOT WORK WITH THE CHANGES IN THIS BRANCH
#sudo cargo build --release --bin=sui
#sudo cargo build --release --bin=sui-faucet

# Using `sudo` here because `cargo flamegraph` only works with `sudo` under macOS
# and messes up the file permissions of the build files.
sudo cargo build --release --bin=simple_channel_executor

# Copy binaries over.

# DON'T BUILD sui AND sui-faucet FROM SOURCE
# BECAUSE THEY CURRENTLY DO NOT WORK WITH THE CHANGES IN THIS BRANCH
#cp ../target/release/sui ./
#cp ../target/release/sui-faucet ./

cp ../target/release/simple_channel_executor ./
