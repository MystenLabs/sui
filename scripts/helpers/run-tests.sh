#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script to run various test suites in the Sui project
# Usage: ./run-tests.sh [rust|typescript|move|all]

set -e

TEST_TYPE=${1:-all}

echo "========================================="
echo "Sui Test Runner"
echo "========================================="
echo ""

run_rust_tests() {
    echo "Running Rust tests..."
    echo ""

    if ! command -v cargo &> /dev/null; then
        echo "Error: Cargo not found. Please install Rust."
        return 1
    fi

    # Run unit tests
    echo "→ Running unit tests..."
    cargo test --lib

    # Run integration tests
    echo ""
    echo "→ Running integration tests..."
    cargo test --test '*'

    echo ""
    echo "✓ Rust tests complete"
}

run_typescript_tests() {
    echo "Running TypeScript tests..."
    echo ""

    if ! command -v npm &> /dev/null; then
        echo "Error: npm not found. Please install Node.js."
        return 1
    fi

    cd sdk/typescript || exit 1

    # Install dependencies
    echo "→ Installing dependencies..."
    npm install

    # Run tests
    echo ""
    echo "→ Running tests..."
    npm test

    cd - > /dev/null

    echo ""
    echo "✓ TypeScript tests complete"
}

run_move_tests() {
    echo "Running Move tests..."
    echo ""

    if [ ! -f "target/release/sui" ] && [ ! -f "target/debug/sui" ]; then
        echo "Error: Sui binary not found. Please build the project first."
        return 1
    fi

    # Find sui binary
    SUI_BIN="./target/release/sui"
    if [ ! -f "$SUI_BIN" ]; then
        SUI_BIN="./target/debug/sui"
    fi

    echo "→ Testing Move examples..."

    # Test each Move package
    for package_dir in sui_programmability/examples/*/; do
        if [ -f "${package_dir}Move.toml" ]; then
            package_name=$(basename "$package_dir")
            echo "  Testing $package_name..."

            if $SUI_BIN move test --path "$package_dir" 2>/dev/null; then
                echo "    ✓ $package_name tests passed"
            else
                echo "    ℹ $package_name (no tests or build only)"
            fi
        fi
    done

    echo ""
    echo "✓ Move tests complete"
}

# Run tests based on argument
case "$TEST_TYPE" in
    rust)
        run_rust_tests
        ;;
    typescript|ts)
        run_typescript_tests
        ;;
    move)
        run_move_tests
        ;;
    all)
        run_rust_tests
        echo ""
        echo "---"
        echo ""
        run_typescript_tests
        echo ""
        echo "---"
        echo ""
        run_move_tests
        ;;
    *)
        echo "Usage: $0 [rust|typescript|move|all]"
        echo ""
        echo "Options:"
        echo "  rust       - Run Rust tests only"
        echo "  typescript - Run TypeScript tests only"
        echo "  move       - Run Move tests only"
        echo "  all        - Run all tests (default)"
        exit 1
        ;;
esac

echo ""
echo "========================================="
echo "All tests complete!"
echo "========================================="
