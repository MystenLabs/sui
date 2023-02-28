#!/bin/bash

NUM_CPUS=$(cat /proc/cpuinfo | grep processor | wc -l) # ubuntu
# NUM_CPUS=$(sysctl -n hw.ncpu) mac

for SUB_SEED in `seq 1 $NUM_CPUS`; do
  SEED="$SUB_SEED$DATE"
  LOG_FILE="log-$SEED"
  echo "Iteration $SUB_SEED using MSIM_TEST_SEED=$SEED, logging to $LOG_FILE"
  # TODO: need to run particular tests with different repeat counts.
  MSIM_TEST_SEED="$SEED" MSIM_TEST_NUM=20 MSIM_WATCHDOG_TIMEOUT_MS=60000 scripts/simtest/cargo-simtest simtest --no-capture > "$LOG_FILE" 2>&1 &
done

for JOB in $(jobs -p); do
  wait $JOB || echo "job failed"
done
