#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Verify simulator determinism by running the determinism tests twice, in two
# separate processes, with the same seed, and checking that the log output is
# identical.
#
# This replaces the old in-process MSIM_TEST_CHECK_DETERMINISM mechanism, which ran a
# test twice within a single process. With the blocking-task pool a simulation now
# spans multiple threads, so per-test state is kept in process globals and there must
# be at most one simulation per process - hence each iteration runs in its own process.
#
# The already-built test binary is run directly (as seed-search.py does), rather than
# via `cargo nextest`: that reuses the binary the surrounding `cargo simtest` job just
# built, without cargo/nextest re-examining and re-linking the whole workspace.
#
# MSIM_TEST_SEED (default 1) selects the seed used for both runs.

set -euo pipefail

# We compare the log output of two runs, so the check is only meaningful for tests that
# emit a rich, deterministic log stream. test_net_determinism (start a network + fullnode,
# send a transaction, sync it) produces ~7k deterministic sim/node log lines - a thorough
# fingerprint of the whole stack. The other former check_determinism tests emit little or
# nothing at this log level (the runtime canaries print only a debug! rng line that
# init_for_testing suppresses), so log-comparison can't check them; running them here would
# just add minutes of CI for no signal.
BINARY_NAME="${BINARY_NAME:-simulator_tests}"
TESTS=(
  test_net_determinism
)
SEED="${MSIM_TEST_SEED:-1}"

ROOT_DIR=$(git rev-parse --show-toplevel)
cd "$ROOT_DIR"

# Locate the newest executable test binary built under the simulator profile.
BIN=""
while IFS= read -r f; do
  case "$f" in *.d) continue;; esac
  [ -x "$f" ] && { BIN="$f"; break; }
done < <(ls -t "$ROOT_DIR"/target/simulator/deps/"${BINARY_NAME}"-* 2>/dev/null)

if [ -z "$BIN" ]; then
  echo "FAIL: could not find an executable '${BINARY_NAME}' binary under target/simulator/deps."
  echo "Build it first, e.g.: scripts/simtest/cargo-simtest simtest --profile ci -p sui-e2e-tests"
  exit 1
fi

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# Reduce two runs to their genuinely-comparable content. Simulation content and event
# order are deterministic; what legitimately differs between two processes is stripped:
# ANSI color, leading log timestamps (sim-clock and any real-clock lines), temp dir
# names, and real OS thread ids.
normalize() {
  sed -E \
    -e 's/\x1b\[[0-9;]*m//g' \
    -e 's/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z?[[:space:]]*//' \
    -e 's/finished in [0-9]+\.[0-9]+s/finished in DURs/g' \
    -e 's#/var/folders/[^ "]*#TMPPATH#g' \
    -e 's#/tmp/[^ "]*#TMPPATH#g' \
    -e 's#tmp\.[A-Za-z0-9]+#TMP#g' \
    -e 's#\.tmp[A-Za-z0-9]+#TMP#g' \
    -e 's#ThreadId\([0-9]+\)#ThreadId(N)#g'
}

run() {
  local out="$1"
  echo "  running ${#TESTS[@]} determinism tests with MSIM_TEST_SEED=$SEED ..."
  set +e
  set -o pipefail
  # RUST_LOG is set explicitly (not inherited): the CI job exports RUST_LOG=error,
  # which would leave almost nothing to compare. The bulk of the comparable, deterministic
  # output comes from test_net_determinism's node/sim logs (sui targets); the other tests
  # run to completion under the same seed and their results are compared too.
  MSIM_TEST_SEED="$SEED" \
  MSIM_WATCHDOG_TIMEOUT_MS="${MSIM_WATCHDOG_TIMEOUT_MS:-60000}" \
  RUST_LOG="sui=debug,info" \
    "$BIN" --test-threads 1 --nocapture --exact "${TESTS[@]}" 2>&1 | normalize > "$out"
  local rc=${PIPESTATUS[0]}
  set +o pipefail
  set -e
  return "$rc"
}

echo "Determinism check on '$(basename "$BIN")': running twice in separate processes, seed=$SEED"
rc1=0; run "$WORK_DIR/run1.log" || rc1=$?
rc2=0; run "$WORK_DIR/run2.log" || rc2=$?

# A test that fails identically twice is still a failure, not "deterministic".
if [ "$rc1" -ne 0 ] || [ "$rc2" -ne 0 ]; then
  echo "FAIL: the tests did not pass (exit codes: $rc1, $rc2); see output above."
  exit 1
fi

# Guard against a run that silently executed nothing (e.g. renamed test).
ran=$(grep -oE 'running [0-9]+ tests?' "$WORK_DIR/run1.log" | grep -oE '[0-9]+' | head -1)
: "${ran:=0}"
echo "  ran $ran test(s)"
if [ "$ran" -ne "${#TESTS[@]}" ]; then
  echo "FAIL: expected ${#TESTS[@]} determinism tests, but ran $ran (check the test names)."
  exit 1
fi

if diff -q "$WORK_DIR/run1.log" "$WORK_DIR/run2.log" >/dev/null; then
  echo "PASS: identical output across two independent runs (seed=$SEED)"
  exit 0
fi

echo "FAIL: simulator output diverged between two runs with the same seed."
echo "This indicates non-determinism. First differences:"
diff "$WORK_DIR/run1.log" "$WORK_DIR/run2.log" | head -80
exit 1
