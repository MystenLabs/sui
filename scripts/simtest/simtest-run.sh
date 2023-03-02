#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

NUM_CPUS=$(cat /proc/cpuinfo | grep processor | wc -l) # ubuntu
# NUM_CPUS=$(sysctl -n hw.ncpu) # mac

DATE=$(date +%s)

# TODO: increase to 30, 1000 respectively after workflow is debugged.
export MSIM_TEST_NUM=30
export SIM_STRESS_TEST_DURATION_SECS=1000

SEED="$DATE"
# LOG_FILE="log-$SEED"

# This command runs many different tests, so it already uses all CPUs fairly efficiently, and
# don't need to be done inside of the for loop below.
# TODO: this logs directly to stdout since it is not being run in parallel. is that ok?
MSIM_TEST_SEED="$SEED" MSIM_WATCHDOG_TIMEOUT_MS=60000 scripts/simtest/cargo-simtest simtest --package sui --package sui-core --profile simtestnightly

for SUB_SEED in `seq 1 $NUM_CPUS`; do
  SEED="$SUB_SEED$DATE"
  #LOG_FILE="log-$SEED"
  echo "Iteration $SUB_SEED using MSIM_TEST_SEED=$SEED" # logging to $LOG_FILE"

  # TODO: currently this only runs one test, (`test_simulated_load_basic`).
  # we need to add `--run-ignored all` in order to enable the ignored tests.
  # However, the ignored tests are totally broken right now because of
  # https://github.com/MystenLabs/sui/pull/8244.
  #
  # Note that because of --no-capture, even though we are running many tests, they will be
  # serialized here. So we still need the for loop / backgrounding.
  MSIM_TEST_SEED="$SEED" MSIM_TEST_NUM=1 MSIM_WATCHDOG_TIMEOUT_MS=60000 scripts/simtest/cargo-simtest simtest --package sui-benchmark --test-threads 1 --profile simtestnightly & # > "$LOG_FILE" 2>&1 &
done

# wait for all the jobs to end
wait
