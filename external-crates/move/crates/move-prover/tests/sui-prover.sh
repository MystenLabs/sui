#!/bin/bash

set -e

# Define dependencies
dotnet_version="8"
rust_toolchain="nightly"
z3_version="latest"
sui_repo="https://github.com/asymptotic-code/sui.git"
boogie_repo="https://github.com/boogie-org/boogie.git"
install_dir="/usr/local/bin"

# Install dependencies
if ! brew list --versions "dotnet@$dotnet_version" &>/dev/null; then
    echo "Installing .NET SDK $dotnet_version..."
    brew install "dotnet@$dotnet_version"
fi

if ! command -v cargo &>/dev/null; then
    echo "Installing Rust..."
    brew install rust
fi

if ! command -v z3 &>/dev/null; then
    echo "Installing Z3..."
    brew install z3
fi

# Build SuiProver
cargo install --locked --path ./crates/sui-prover
mkdir -p "$install_dir"
mv "$HOME/.cargo/bin/sui-prover" "$install_dir/"

# Set up .NET environment
export DOTNET_ROOT="$(brew --prefix dotnet@$dotnet_version)/libexec"
export PATH="$DOTNET_ROOT:$PATH"

# Clone and build Boogie
echo "Cloning Boogie repository..."
git clone --branch master "$boogie_repo" boogie-src
cd boogie-src

dotnet build Source/Boogie.sln -c Release
mv Source/BoogieDriver/bin/Release/net8.0/BoogieDriver "$install_dir/boogie"

cd ..

export BOOGIE_EXE="$install_dir/boogie"
export Z3_EXE="/usr/local/bin/z3"

# Verify installation
echo "Verifying installation..."
sui-prover --version

echo "Installation complete. The formal verification toolchain is ready."

cd ./crates/sui-framework/packages/prover
sui-prover
