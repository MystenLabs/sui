#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "Running simulator tests at commit $(git rev-parse HEAD)"

# Function to handle SIGINT signal (Ctrl+C)
cleanup() {
    echo "Cleaning up child processes..."
    # Kill all child processes in the process group of the current script
    kill -- "-$$"
    exit 1
}

# Set up the signal handler
trap cleanup SIGINT

if [ -z "$NUM_CPUS" ]; then
  NUM_CPUS=$(cat /proc/cpuinfo | grep processor | wc -l) # ubuntu
fi

# filter out some tests that give spurious failures.
TEST_FILTER="(not (test(~batch_verification_tests)))"

DATE=$(date +%s)
SEED="$DATE"

# create logs directory
SIMTEST_LOGS_DIR=~/simtest_logs
[ ! -d ${SIMTEST_LOGS_DIR} ] && mkdir -p ${SIMTEST_LOGS_DIR}
[ ! -d ${SIMTEST_LOGS_DIR}/${DATE} ] && mkdir -p ${SIMTEST_LOGS_DIR}/${DATE}

LOG_DIR="${SIMTEST_LOGS_DIR}/${DATE}"
LOG_FILE="$LOG_DIR/log"

# By default run 1 iteration for each test, if not specified.
: ${TEST_NUM:=1}

echo ""
echo "================================================"
echo "Running e2e simtests with $TEST_NUM iterations"
echo "================================================"
date

# This command runs many different tests, so it already uses all CPUs fairly efficiently, and
# don't need to be done inside of the for loop below.
# TODO: this logs directly to stdout since it is not being run in parallel. is that ok?
MSIM_TEST_SEED="$SEED" \
MSIM_TEST_NUM=${TEST_NUM} \
MSIM_WATCHDOG_TIMEOUT_MS=60000 \
scripts/simtest/cargo-simtest simtest \
  --color always \
  --test-threads "$NUM_CPUS" \
  --package sui-core \
  --package sui-archival \
  --package sui-e2e-tests \
  --profile simtestnightly \
  -E "$TEST_FILTER" 2>&1 | tee "$LOG_FILE"

echo ""
echo "============================================="
echo "Running $NUM_CPUS stress simtests in parallel"
echo "============================================="
date

for SUB_SEED in `seq 1 $NUM_CPUS`; do
  SEED="$SUB_SEED$DATE"
  LOG_FILE="$LOG_DIR/log-$SEED"
  echo "Iteration $SUB_SEED using MSIM_TEST_SEED=$SEED, logging to $LOG_FILE"

  # --test-threads 1 is important: parallelism is achieved via the for loop
  MSIM_TEST_SEED="$SEED" \
  MSIM_TEST_NUM=1 \
  MSIM_WATCHDOG_TIMEOUT_MS=60000 \
  SIM_STRESS_TEST_DURATION_SECS=300 \
  scripts/simtest/cargo-simtest simtest \
    --color always \
    --package sui-benchmark \
    --test-threads 1 \
    --profile simtestnightly \
    > "$LOG_FILE" 2>&1 &

done

# wait for all the jobs to end
wait

echo ""
echo "==========================="
echo "Running determinism simtest"
echo "==========================="
date

# Check for determinism in stress simtests
LOG_FILE="$LOG_DIR/determinism-log"
echo "Using MSIM_TEST_SEED=$SEED, logging to $LOG_FILE"

MSIM_TEST_SEED="$SEED" \
MSIM_TEST_NUM=1 \
MSIM_WATCHDOG_TIMEOUT_MS=60000 \
MSIM_TEST_CHECK_DETERMINISM=1 \
scripts/simtest/cargo-simtest simtest \
  --color always \
  --test-threads "$NUM_CPUS" \
  --package sui-benchmark \
  --profile simtestnightly \
  -E "$TEST_FILTER" 2>&1 | tee "$LOG_FILE"

echo ""
echo "============================================="
echo "All tests completed, checking for failures..."
echo "============================================="
date

grep -EqHn 'TIMEOUT|FAIL' "$LOG_DIR"/*

# if grep found no failures exit now
[ $? -eq 1 ] && echo "No test failures detected" && exit 0

echo "Failures detected, printing logs..."

# read all filenames in $LOG_DIR that contain the string "FAIL" into a bash array
# and print the line number and filename for each
readarray -t FAILED_LOG_FILES < <(grep -El 'TIMEOUT|FAIL' "$LOG_DIR"/*)

# iterate over the array and print the contents of each file
for LOG_FILE in "${FAILED_LOG_FILES[@]}"; do
  echo ""
  echo "=============================="
  echo "Failure detected in $LOG_FILE:"
  echo "=============================="
  cat "$LOG_FILE"
done

exit 1
