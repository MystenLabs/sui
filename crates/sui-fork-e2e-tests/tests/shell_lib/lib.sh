#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Shared helpers for the sui-fork shell tests. The harness copies this file into the sandbox root,
# and every script starts with `set -euo pipefail` followed by `source ./lib.sh`.
#
# The helpers stay compatible with the bash 3.2 that macOS ships, so there are no associative
# arrays, no mapfile, and no timeout(1). They are also written around these traps:
#
# - Under pipefail, `producer | head -1` kills the producer with SIGPIPE and aborts the script, so
#   a stream is cut with `awk ... exit` or jq's first() instead.
# - `local x=$(cmd)` masks the exit status of cmd, so declarations and assignments are separate.
# - `set -e` never fires on a backgrounded command, so fork_start detects an early exit of
#   `sui-fork start` by polling `kill -0`.
# - `sui-fork start` prints its summary before its RPC server binds, so readiness is a `status`
#   call that succeeds rather than a line in the log.
# - `--gas` and `--args` on `sui client` take one or more values, so only a following `--flag`
#   terminates them.

: "${LOCALNET_CONFIG:?LOCALNET_CONFIG must point at the localnet client.yaml}"
: "${FORK_CONFIG:?FORK_CONFIG must be a scratch path for the fork client.yaml}"
: "${GRAPHQL_URL:?GRAPHQL_URL must be the localnet GraphQL endpoint}"
: "${FORK_DATA_DIR:?FORK_DATA_DIR must be an empty scratch directory}"
: "${FORK_RPC_ADDR:?FORK_RPC_ADDR must be a free host:port for the fork RPC server}"

# Keep foreground sui and sui-fork commands silent, so only deliberate output reaches the snapshot.
export RUST_LOG="${RUST_LOG:-error}"
: "${FORK_START_TIMEOUT:=120}"
: "${GRAPHQL_TIMEOUT:=120}"
: "${FORK_LOG:=fork.log}"

FORK_PID=""
FORK_RPC=""
FORK_URL=""
FORK_CHECKPOINT=""
FAILED=0

# ---------------------------------------------------------------------------------- assertions

ok() { echo "ok: $*"; }
fail() { echo "FAIL: $*"; FAILED=1; }

assert_eq() {
  if [ "$1" = "$2" ]; then ok "$3"; else fail "$3 (got '$1', expected '$2')"; fi
}
assert_ne() {
  if [ "$1" != "$2" ]; then ok "$3"; else fail "$3 (both '$1')"; fi
}
assert_gt() {
  if [ "$1" -gt "$2" ]; then ok "$3"; else fail "$3 ($1 is not greater than $2)"; fi
}
assert_nonempty() {
  if [ -n "$1" ]; then ok "$2"; else fail "$2 (empty)"; fi
}
# assert_grep <pattern> <file> <label>: the file is only printed when the pattern is missing.
assert_grep() {
  if grep -q -- "$1" "$2"; then ok "$3"; else fail "$3 (no '$1' in $2)"; cat "$2"; fi
}

# ----------------------------------------------------------------------------------- redaction

# Replace object ids and base58 digests with markers.
redact_ids() {
  sed -E \
    -e 's/0x[0-9a-fA-F]{64}/<ID>/g' \
    -e 's/(^|[^0-9A-Za-z])[1-9A-HJ-NP-Za-km-z]{43,44}($|[^0-9A-Za-z])/\1<DIGEST>\2/g'
}

# Replace ids, digests, "YYYY-MM-DD HH:MM:SS UTC" timestamps, and then every remaining number.
scrub() {
  redact_ids | sed -E \
    -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2} UTC/<TS>/g' \
    -e 's/[0-9]+/<N>/g'
}

# -------------------------------------------------------------------------------- CLI wrappers

on_localnet() { sui client --client.config "$LOCALNET_CONFIG" "$@"; }
on_fork() { sui client --client.config "$FORK_CONFIG" "$@"; }

# fork_cmd <subcommand> [flags]: a sui-fork client command against the running fork, as JSON.
fork_cmd() { sui-fork --json "$@" --rpc-addr "$FORK_RPC"; }
fork_status_field() { fork_cmd status | jq -r ".$1"; }

# The JSON block that `start --json` printed, taken from the fork log.
fork_start_json() { sed -n '/^{$/,/^}$/p' "$FORK_LOG"; }

# run_json <out.json> <cmd...>: run a command with --json, stdout to the file and stderr next to
# it, printing both only when the command fails.
run_json() {
  local out=$1
  shift
  if ! "$@" --json > "$out" 2> "$out.err"; then
    echo "command failed: $*"
    cat "$out.err" "$out"
    return 1
  fi
}

# ------------------------------------------------------------------------------ fork lifecycle

# _fork_spawn [--json] [start flags]: background `sui-fork start` on FORK_RPC_ADDR with its data
# under FORK_DIR (default FORK_DATA_DIR), then wait until its RPC server answers.
_fork_spawn() {
  local json_flag=""
  if [ "${1:-}" = "--json" ]; then
    json_flag="--json"
    shift
  fi
  local dir="${FORK_DIR:-$FORK_DATA_DIR}"
  mkdir -p "$dir"
  : > "$FORK_LOG"
  FORK_RPC="$FORK_RPC_ADDR"
  FORK_URL="http://$FORK_RPC"
  # The fork's output goes to a file rather than to the script's stdout, because a background
  # process holding the harness's pipe would keep the harness waiting after the script exits.
  # shellcheck disable=SC2086
  RUST_LOG="${FORK_RUST_LOG:-info}" sui-fork $json_flag start \
    --network "$GRAPHQL_URL" --rpc-addr "$FORK_RPC" --data-dir "$dir" "$@" \
    > "$FORK_LOG" 2>&1 < /dev/null &
  FORK_PID=$!
  trap fork_stop EXIT

  local waited=0
  until sui-fork status --rpc-addr "$FORK_RPC" > /dev/null 2>&1; do
    if ! kill -0 "$FORK_PID" 2> /dev/null; then
      echo "sui-fork exited before its RPC server came up:" >&2
      cat "$FORK_LOG" >&2
      FORK_PID=""
      return 1
    fi
    if [ "$waited" -ge $((FORK_START_TIMEOUT * 5)) ]; then
      echo "timed out after ${FORK_START_TIMEOUT}s waiting for sui-fork to start:" >&2
      cat "$FORK_LOG" >&2
      fork_stop
      return 1
    fi
    sleep 0.2
    waited=$((waited + 1))
  done
  export FORK_PID FORK_RPC FORK_URL
}

# fork_start [start flags]: start a fork with `--json` and export FORK_RPC, FORK_URL, and
# FORK_CHECKPOINT (the fork point that `start` reported).
fork_start() {
  _fork_spawn --json "$@"
  FORK_CHECKPOINT=$(awk '/"checkpoint": / { gsub(/[^0-9]/, ""); print; exit }' "$FORK_LOG")
  if [ -z "$FORK_CHECKPOINT" ]; then
    echo "could not parse the fork checkpoint from $FORK_LOG:" >&2
    cat "$FORK_LOG" >&2
    return 1
  fi
  export FORK_CHECKPOINT
}

# fork_start_plain [start flags]: start a fork without `--json`, so the log holds the
# human-readable "Starting" or "Resuming" line.
fork_start_plain() {
  _fork_spawn "$@"
  FORK_CHECKPOINT=$(fork_status_field forked_at_checkpoint)
  export FORK_CHECKPOINT
}

# fork_stop: stop the fork with SIGTERM, escalating to SIGKILL after ten seconds. Idempotent and
# never fails, because it also runs from the EXIT trap.
fork_stop() {
  if [ -n "${FORK_PID:-}" ] && kill -0 "$FORK_PID" 2> /dev/null; then
    kill -TERM "$FORK_PID" 2> /dev/null || true
    local waited=0
    while kill -0 "$FORK_PID" 2> /dev/null && [ "$waited" -lt 100 ]; do
      sleep 0.1
      waited=$((waited + 1))
    done
    if kill -0 "$FORK_PID" 2> /dev/null; then
      kill -KILL "$FORK_PID" 2> /dev/null || true
    fi
    wait "$FORK_PID" 2> /dev/null || true
  fi
  FORK_PID=""
}

# fork_env: recreate FORK_CONFIG from the localnet config, add the running fork as env `fork`, and
# make it the active env. Recreating the file lets a script start several forks in a row, because
# `new-env` refuses a duplicate alias.
fork_env() {
  cp "$LOCALNET_CONFIG" "$FORK_CONFIG"
  if ! on_fork new-env --alias fork --rpc "$FORK_URL" > env.log 2>&1; then
    cat env.log
    return 1
  fi
  if ! on_fork switch --env fork >> env.log 2>&1; then
    cat env.log
    return 1
  fi
}

# ------------------------------------------------------------------------------------- GraphQL

# graphql <query>: POST a query to the localnet GraphQL endpoint and print the raw response.
graphql() {
  curl -sS -X POST -H 'Content-Type: application/json' \
    --data "$(jq -cn --arg q "$1" '{query: $q}')" "$GRAPHQL_URL"
}

latest_localnet_checkpoint() {
  graphql '{ checkpoint { sequenceNumber } }' | jq -r '.data.checkpoint.sequenceNumber // empty'
}

# wait_for_graphql <checkpoint>: wait until GraphQL has indexed the checkpoint.
wait_for_graphql() {
  local target=$1
  local waited=0
  local latest=""
  while :; do
    latest=$(latest_localnet_checkpoint 2> /dev/null || true)
    if [ -n "$latest" ] && [ "$latest" -ge "$target" ]; then
      return 0
    fi
    if [ "$waited" -ge $((GRAPHQL_TIMEOUT * 2)) ]; then
      echo "timed out waiting for GraphQL to reach checkpoint $target (latest '$latest')" >&2
      return 1
    fi
    sleep 0.5
    waited=$((waited + 1))
  done
}

# wait_for_graphql_tx <digest>: wait until GraphQL has indexed the transaction, then print the
# checkpoint that finalised it.
wait_for_graphql_tx() {
  local digest=$1
  local waited=0
  local checkpoint=""
  while :; do
    checkpoint=$(graphql "{ transaction(digest: \"$digest\") { effects { checkpoint { sequenceNumber } } } }" \
      | jq -r '.data.transaction.effects.checkpoint.sequenceNumber // empty' 2> /dev/null || true)
    if [ -n "$checkpoint" ]; then
      echo "$checkpoint"
      return 0
    fi
    if [ "$waited" -ge $((GRAPHQL_TIMEOUT * 2)) ]; then
      echo "timed out waiting for GraphQL to index transaction $digest" >&2
      return 1
    fi
    sleep 0.5
    waited=$((waited + 1))
  done
}

# ---------------------------------------------------------------- sui client --json helpers

# Where a helper takes <where>, pass on_localnet or on_fork.

# other_address <address>: some keystore address other than the given one.
other_address() {
  on_localnet addresses --json | jq -r --arg a "$1" '[.addresses[] | select(.[1] != $a) | .[1]][0]'
}

gas_coin() { "$1" gas ${2:+"$2"} --json | jq -r '.gasCoins[0].gasCoinId'; }
gas_count() { "$1" gas ${2:+"$2"} --json | jq -r '.gasCoins | length'; }
# gas_total <where> [address]: the MIST held in gas coins, summed in bash because jq prints sums
# past 17 digits in scientific notation, which bash arithmetic cannot read.
gas_total() {
  local total=0
  local balance
  for balance in $("$1" gas ${2:+"$2"} --json | jq -r '.gasCoins[].mistBalance'); do
    total=$((total + balance))
  done
  echo "$total"
}

# object_field <where> <object id> <jq expression>
object_field() { "$1" object "$2" --json | jq -r "$3"; }
# object_bcs_field <where> <object id> <jq expression>: the same over `object --bcs`, whose
# `.data.Move.contents` holds the raw BCS bytes.
object_bcs_field() { "$1" object "$2" --bcs --json | jq -r "$3"; }

tx_digest_of() { jq -r '.digest' "$1"; }
tx_status_of() { jq -r '.effects.status.status' "$1"; }
published_package_id() {
  jq -r 'first(.objectChanges[] | select(.type == "published")) | .packageId' "$1"
}
# created_object_id <tx.json> <type suffix>
created_object_id() {
  jq -r --arg t "$2" \
    'first(.objectChanges[] | select(.type == "created" and (.objectType | endswith($t)))) | .objectId' \
    "$1"
}
# mutated_object_id <tx.json> <type suffix>
mutated_object_id() {
  jq -r --arg t "$2" \
    'first(.objectChanges[] | select(.type == "mutated" and (.objectType | endswith($t)))) | .objectId' \
    "$1"
}

# add_env_to_toml <package dir> <env alias> <where>: append an [environments] entry with the chain
# identifier of that network, which `sui client publish` needs to find the environment.
add_env_to_toml() {
  printf '\n[environments]\n%s = "%s"\n' "$2" "$("$3" chain-identifier --format=hex)" >> "$1/Move.toml"
}

# extract_published <Published.toml>: only the environment header and the version, because every
# other line holds an id that changes per run.
extract_published() {
  awk '
    /^\[published\.[^]]+\]/ { print; inpub = 1; next }
    inpub && /^version[[:space:]]*=/ { print; inpub = 0 }
  ' "$1"
}
