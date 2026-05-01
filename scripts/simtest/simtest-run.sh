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
PHASE1_EXIT=${PIPESTATUS[0]}

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

# Phase 1 is failed if any individual test failed (`failures.ndjson` non-empty)
# OR if seed-search.py itself exited non-zero (e.g. build failure, infrastructure
# error). The pipe through tee otherwise hides that exit code.
if [ -s "$LOG_DIR/e2e/failures.ndjson" ] || [ "${PHASE1_EXIT:-0}" -ne 0 ]; then
  PHASE1_FAILED=1
fi

# Phase 2/3 logs are flat files in $LOG_DIR (log-* per stress iteration plus
# determinism-log). Phase 1's per-job logs live under $LOG_DIR/e2e/, which we
# intentionally don't grep over here — Phase 1 failures are surfaced via
# failures.ndjson.
#
# TODO: this regex misses signal-based terminations. nextest reports
# signal-killed tests with status tokens other than FAIL/TIMEOUT — e.g.
# `SIGABRT [time] pkg::bin::test`, and similarly SIGSEGV, SIGBUS, SIGKILL,
# SIGTRAP, SIGFPE, SIGSYS, plus LEAK for goroutine/thread leaks. Today
# sui-benchmark's `test_simulated_load_large_consensus_commit_prologue_size`
# SIGABRTs in ~50% of stress iterations on main and silently passes as a
# result. Same regex is used in collect-failures.sh — keep them in sync.
if grep -EqHn 'TIMEOUT|FAIL' "$LOG_DIR"/log-* "$LOG_DIR"/determinism-log 2>/dev/null; then
  PHASE23_FAILED=1
fi

if [ "$PHASE1_FAILED" -eq 0 ] && [ "$PHASE23_FAILED" -eq 0 ]; then
  echo "No test failures detected"
  exit 0
fi

echo "Failures detected, printing details..."

# Build/infra failure case (seed-search.py exited non-zero, e.g. cargo build
# error). The relevant evidence is in $LOG_DIR/log; surface its tail before
# the per-test rendering.
if [ "${PHASE1_EXIT:-0}" -ne 0 ]; then
  echo ""
  echo "=============================="
  echo "Phase 1 build/infrastructure failure:"
  echo "=============================="
  echo "seed-search.py exited with code $PHASE1_EXIT"
  echo "Tail of $LOG_DIR/log:"
  tail -100 "$LOG_DIR/log"
fi

# Per-test failure rendering (Phase 1 NDJSON + Phase 2/3 nextest plaintext)
# lives in collect-failures.py.
scripts/simtest/collect-failures.py --format=detailed "$LOG_DIR"

exit 1
