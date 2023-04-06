#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

if [ -z "$NUM_CPUS" ]; then
  NUM_CPUS=$(cat /proc/cpuinfo | grep processor | wc -l) # ubuntu
fi

# filter out some tests that give spurious failures.
TEST_FILTER="(not test(~cli_tests))"

DATE=$(date +%s)
SEED="$DATE"
# LOG_FILE="log-$SEED"

# This command runs many different tests, so it already uses all CPUs fairly efficiently, and
# don't need to be done inside of the for loop below.
# TODO: this logs directly to stdout since it is not being run in parallel. is that ok?
MSIM_TEST_SEED="$SEED" \
MSIM_WATCHDOG_TIMEOUT_MS=60000 \
MSIM_TEST_NUM=30 \
scripts/simtest/cargo-simtest simtest \
  --package sui \
  --test-threads "$NUM_CPUS" \
  --package sui-core \
  --profile simtestnightly \
  -E "$TEST_FILTER"

# create logs directory
SIMTEST_LOGS_DIR=~/simtest_logs
[ ! -d ${SIMTEST_LOGS_DIR} ] && mkdir -p ${SIMTEST_LOGS_DIR}
[ ! -d ${SIMTEST_LOGS_DIR}/${DATE} ] && mkdir -p ${SIMTEST_LOGS_DIR}/${DATE}

for SUB_SEED in `seq 1 $NUM_CPUS`; do
  SEED="$SUB_SEED$DATE"
  LOG_FILE=${SIMTEST_LOGS_DIR}/${DATE}/"log-$SEED"
  echo "Iteration $SUB_SEED using MSIM_TEST_SEED=$SEED, logging to $LOG_FILE"

  # --test-threads 1 is important: parallelism is achieved via the for loop
  MSIM_TEST_SEED="$SEED" \
  MSIM_TEST_NUM=1 \
  MSIM_WATCHDOG_TIMEOUT_MS=60000 \
  SIM_STRESS_TEST_DURATION_SECS=300 \
  scripts/simtest/cargo-simtest simtest \
    --package sui-benchmark \
    --test-threads 1 \
    --profile simtestnightly \
    -E "$TEST_FILTER" > "$LOG_FILE" 2>&1 &

  grep -Hn FAIL "$LOG_FILE"
done

# wait for all the jobs to end
wait
