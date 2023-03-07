#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

NUM_CPUS=$(cat /proc/cpuinfo | grep processor | wc -l) # ubuntu
# NUM_CPUS=$(sysctl -n hw.ncpu) # mac

# filter out some tests that give spurious failures.
TEST_FILTER="(not test(test_move_call_args_linter_command)) & (not test(test_package_publish_command))"

DATE=$(date +%s)

export MSIM_TEST_NUM=30
export SIM_STRESS_TEST_DURATION_SECS=300
export CARGO_TERM_COLOR=always
export CARGO_INCREMENTAL=0
export CARGO_NET_RETRY=10
export RUSTUP_MAX_RETRIES=10
export RUST_BACKTRACE=short
export RUST_LOG=off
# Runs tests much faster - disables signing and verification
export USE_MOCK_CRYPTO=1

SEED="$DATE"
# LOG_FILE="log-$SEED"

# This command runs many different tests, so it already uses all CPUs fairly efficiently, and
# don't need to be done inside of the for loop below.
# TODO: this logs directly to stdout since it is not being run in parallel. is that ok?
MSIM_TEST_SEED="$SEED" \
MSIM_WATCHDOG_TIMEOUT_MS=60000 \
scripts/simtest/cargo-simtest simtest \
  --package sui \
  --package sui-core \
  --profile simtestnightly

for SUB_SEED in `seq 1 $NUM_CPUS`; do
  SEED="$SUB_SEED$DATE"
  echo "Iteration $SUB_SEED using MSIM_TEST_SEED=$SEED"

  # --test-threads 1 is important: parallelism is achieved via the for loop
  MSIM_TEST_SEED="$SEED" \
  MSIM_TEST_NUM=1 \
  MSIM_WATCHDOG_TIMEOUT_MS=60000 \
  scripts/simtest/cargo-simtest simtest \
    --package sui-benchmark \
    --run-ignored all \
    --test-threads 1 \
    --profile simtestnightly \
    -E "$TEST_FILTER" &
done

# wait for all the jobs to end
wait
