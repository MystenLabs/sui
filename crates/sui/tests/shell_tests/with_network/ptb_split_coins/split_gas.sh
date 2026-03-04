#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

run_ptb() {
  local output=$1
  shift
  if ! sui client --client.config "$CONFIG" ptb \
    --move-call sui::tx_context::sender \
    --assign sender \
    --split-coins gas "[1000]" \
    --assign split_coin \
    --transfer-objects "[split_coin]" sender \
    --gas-budget 50000000 \
    "$@" \
    > "$output" 2>&1; then
    cat "$output"
    exit 1
  fi
}

redact_output() {
  perl -pe '
    s/0x[0-9a-fA-F]{64}/"0x" . ("X" x 64)/eg;

    s/\b([1-9A-HJ-NP-Za-km-z]{32,})\b/"D" x length($1)/eg;
    s/\b([A-Za-z0-9+\/]{80,}={0,2})\b/"S" x length($1)/eg;

    s/"D{32,}"/"<DIGEST>"/g;
    s/"S{80,}"/"<SIGNATURE>"/g;
    s/(Transaction Digest:\s+)D+/$1<DIGEST>/g;
    s/(Digest:\s+)([^│\n]+)(│)/$1 . "<DIGEST>" . (" " x (length($2) > 8 ? length($2) - 8 : 0)) . $3/eg;

    s/(\bExecuted Epoch:\s*)(\d+)/$1 . ("0" x length($2))/eg;
    s/("executedEpoch":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("timestampMs":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("checkpoint":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;

    s/(\bVersion:\s*)(\d+)/$1 . ("0" x length($2))/eg;
    s/("version":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("version":\s*)(\d+)(,?)/$1 . ("0" x length($2)) . $3/eg;
    s/("previousVersion":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("sequenceNumber":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;

    s/(Estimated gas cost \(includes a small buffer\):\s*)(\d+)( MIST)/$1 . ("0" x length($2)) . $3/eg;
    s/(\bStorage Cost:\s*)(\d+)( MIST)/$1 . ("0" x length($2)) . $3/eg;
    s/(\bComputation Cost:\s*)(\d+)( MIST)/$1 . ("0" x length($2)) . $3/eg;
    s/(\bStorage Rebate:\s*)(\d+)( MIST)/$1 . ("0" x length($2)) . $3/eg;
    s/(\bNon-refundable Storage Fee:\s*)(\d+)( MIST)/$1 . ("0" x length($2)) . $3/eg;
    s/(\bAmount:\s*)(-?)(\d+)/$1 . $2 . ("0" x length($3))/eg;

    s/("computationCost":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("storageCost":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("storageRebate":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("nonRefundableStorageFee":\s*")(\d+)(")/$1 . ("0" x length($2)) . $3/eg;
    s/("amount":\s*")(-?)(\d+)(")/$1 . $2 . ("0" x length($3)) . $4/eg;
  ' "$@"
}

echo "=== dry-run ==="
run_ptb dry-run.log --dry-run
redact_output dry-run.log

echo
echo "=== execute ==="
run_ptb execute.log
redact_output execute.log

echo
echo "=== json ==="
run_ptb json.log --json
jq '
  .objectChanges |= sort_by(.type, .objectId)
  | .effects.created |= sort_by(.reference.objectId)
  | .effects.mutated |= sort_by(.reference.objectId)
  | .effects.modifiedAtVersions |= sort_by(.objectId)
' json.log | redact_output
