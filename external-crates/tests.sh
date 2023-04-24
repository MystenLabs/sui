#!/bin/sh

# Fail after the first error
set -e

# Run tests for move
echo "Running Move tests in external-crates"
echo "Excluding prover and evm Move tests"

cargo nextest run                     \
      --manifest-path move/Cargo.toml \
      -E '!(package(move-to-yul) | package(move-prover) | test(prove) | test(simple_build_with_docs))'

# Run tests for various versions of move-execution
for v in move-execution/v*; do
    echo "Running Move execution $(basename "$v") tests"
    cargo nextest run --manifest-path "$v/Cargo.toml"
done
