#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script to set up a local Sui development environment
# Usage: ./setup-dev-env.sh

set -e

echo "========================================="
echo "Sui Development Environment Setup"
echo "========================================="
echo ""

# Check prerequisites
echo "Checking prerequisites..."

# Check for Rust
if ! command -v rustc &> /dev/null; then
    echo "✗ Rust not found. Please install Rust from https://rustup.rs/"
    exit 1
fi
echo "✓ Rust: $(rustc --version)"

# Check for Cargo
if ! command -v cargo &> /dev/null; then
    echo "✗ Cargo not found. Please install Rust from https://rustup.rs/"
    exit 1
fi
echo "✓ Cargo: $(cargo --version)"

# Check for Git
if ! command -v git &> /dev/null; then
    echo "✗ Git not found. Please install Git first."
    exit 1
fi
echo "✓ Git: $(git --version)"

echo ""
echo "Building Sui binaries..."
echo "This may take several minutes..."

# Build Sui
cargo build --release

echo ""
echo "✓ Build complete"
echo ""

# Set up configuration directory
CONFIG_DIR="$HOME/.sui/sui_config"
if [ ! -d "$CONFIG_DIR" ]; then
    echo "Creating configuration directory..."
    mkdir -p "$CONFIG_DIR"
    echo "✓ Configuration directory created: $CONFIG_DIR"
else
    echo "✓ Configuration directory exists: $CONFIG_DIR"
fi

echo ""
echo "Initializing Sui client..."

# Initialize client configuration
if [ -f "$CONFIG_DIR/client.yaml" ]; then
    echo "Client configuration already exists"
    read -p "Do you want to reset it? (y/n) " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f "$CONFIG_DIR/client.yaml"
        ./target/release/sui client
    fi
else
    ./target/release/sui client
fi

echo ""
echo "========================================="
echo "Setup Complete!"
echo "========================================="
echo ""
echo "You can now use the following commands:"
echo "  ./target/release/sui client           - Interact with Sui network"
echo "  ./target/release/sui move              - Build Move packages"
echo "  ./target/release/sui start             - Start local network"
echo ""
echo "For more information, visit: https://docs.sui.io"
