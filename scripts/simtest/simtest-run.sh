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

# Phase 1 runs every (test, seed) pair in its own OS process via seed-search.py.
# Running each iteration in a fresh process avoids the bug where a failure in
# iteration N can be masked by intra-process state left over from iterations 1..N-1
# (the previous `MSIM_TEST_NUM=$TEST_NUM` flow looped within a single process).
mkdir -p "$LOG_DIR/e2e"

scripts/simtest/seed-search.py \
  --package consensus-simtests \
  --package sui-core \
  --package sui-e2e-tests \
  --num-seeds "$TEST_NUM" \
  --seed-start "$SEED" \
  --concurrency "$NUM_CPUS" \
  --watchdog-timeout-ms 60000 \
  --exclude 'batch_verification_tests' \
  --log-dir "$LOG_DIR/e2e" \
  --no-reachability 2>&1 | tee "$LOG_FILE"

# Clean up temp files from the e2e phase to prevent /tmp (tmpfs) from filling up.
rm -rf /tmp/tmp.* /tmp/.tmp* /tmp/sui-* 2>/dev/null

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

# Clean up temp files from the stress phase before running determinism tests.
rm -rf /tmp/tmp.* /tmp/.tmp* /tmp/sui-* 2>/dev/null

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

PHASE1_FAILED=0
PHASE23_FAILED=0

if [ -s "$LOG_DIR/e2e/failures.ndjson" ]; then
  PHASE1_FAILED=1
fi

# Phase 2/3 logs are flat files in $LOG_DIR (log-* per stress iteration plus
# determinism-log). Phase 1's per-job logs live under $LOG_DIR/e2e/, which we
# intentionally don't grep over here — Phase 1 failures are surfaced via
# failures.ndjson.
if grep -EqHn 'TIMEOUT|FAIL' "$LOG_DIR"/log-* "$LOG_DIR"/determinism-log 2>/dev/null; then
  PHASE23_FAILED=1
fi

if [ "$PHASE1_FAILED" -eq 0 ] && [ "$PHASE23_FAILED" -eq 0 ]; then
  echo "No test failures detected"
  exit 0
fi

echo "Failures detected, printing details..."

if [ "$PHASE1_FAILED" -eq 1 ]; then
  echo ""
  echo "=============================="
  echo "Phase 1 failures (failures.ndjson):"
  echo "=============================="
  jq -r '"  \(.status) \(.binary)::\(.test) seed=\(.seed) (log: \(.log))"' \
    "$LOG_DIR/e2e/failures.ndjson"
fi

if [ "$PHASE23_FAILED" -eq 1 ]; then
  readarray -t FAILED_LOG_FILES < <(grep -El 'TIMEOUT|FAIL' "$LOG_DIR"/log-* "$LOG_DIR"/determinism-log 2>/dev/null)
  for LOG_FILE in "${FAILED_LOG_FILES[@]}"; do
    echo ""
    echo "=============================="
    echo "Failure detected in $LOG_FILE:"
    echo "=============================="
    cat "$LOG_FILE"
  done
fi

exit 1
