# Copyright (c) Facebook, Inc. and its affiliates.
#!/bin/bash
#
# NOTE: Create an AWS instance with enough SSD storage in the primary
# partitions otherwise RockDB won't run (100GB is more than enough).

sudo apt update
sudo apt -y upgrade
sudo apt -y autoremove

# The following dependencies prevent the error: [error: linker `cc` not found]
sudo apt -y install build-essential
sudo apt -y install cmake

# Install rust (non-interactive)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
rustup default stable

# This is missing from the RockDB installer (needed for RockDB).
sudo apt install -y clang

# Install the repo.
ssh-keyscan github.com >> ~/.ssh/known_hosts
git clone git@github.com:novifinancial/mempool-research.git
cd mempool-research/rust/bench_worker
cargo build --release
cp ../target/release/bench_worker ~/
cd
