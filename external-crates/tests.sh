#!/bin/bash
# --- CI/CD Infrastructure Setup ---
# set -e: Exit immediately if a command exits with a non-zero status
set -e

[Image of CI/CD pipeline for blockchain smart contracts]

# Ensure we are in the correct context
echo "[INFO] Navigating to 'move' directory for external-crate validation..."
cd move || { echo "[ERROR] Directory 'move' not found!"; exit 1; }

# --- Optimized Test Execution (cargo-nextest) ---
# We use nextest for parallel execution and better retry logic than standard cargo test.
# -E: Filters out specific flaky tests or known issues in the prover environment.
# --retries 3: Automatically retries failed tests to distinguish between flakiness and real bugs.

echo "[STEP 1] Running workspace Move tests (Excluding prover-specific edge cases)..."

cargo nextest run \
    -E '!test(run_all::simple_build_with_docs/args.txt) and !test(run_test::nested_deps_bad_parent/Move.toml)' \
    --workspace \
    --no-fail-fast \
    --retries 3

[Image of Parallel test execution architecture in cargo-nextest]

# --- Feature-Specific Validation ---
# Tracing is critical for debugging complex VM interactions and resource flows.
echo "[STEP 2] Running specialized tracing tests for move-cli..."

cargo nextest run -p move-cli --features tracing

[Image of Distributed tracing and logging flow in blockchain node]

echo "[SUCCESS] All external-crate tests passed!"
