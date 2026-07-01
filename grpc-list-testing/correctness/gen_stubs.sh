#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Regenerate Python proto stubs for the v2alpha LedgerService correctness harness.
# Stubs land in ./sui_pb (gitignored). Re-run after bumping the pinned sui-rpc rev
# (keep it in lockstep with ../manifest.*.json "proto_rev").
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/sui_pb"

# Locate the pinned sui-rpc proto roots from the cargo git checkout.
# Override with SUI_RPC_PROTO=/abs/path/to/crates/sui-rpc if discovery fails.
ROOT="${SUI_RPC_PROTO:-}"
if [[ -z "$ROOT" ]]; then
  for d in "$HOME"/.cargo/git/checkouts/sui-rust-sdk-*/*/crates/sui-rpc; do
    if [[ -f "$d/proto/sui/rpc/v2alpha/ledger_service.proto" ]]; then ROOT="$d"; break; fi
  done
fi
[[ -n "$ROOT" && -d "$ROOT" ]] || { echo "ERROR: sui-rpc proto root not found; set SUI_RPC_PROTO" >&2; exit 1; }

ALPHA="$ROOT/proto"        # sui/rpc/v2alpha/*
V2="$ROOT/vendored/proto"  # sui/rpc/v2/* + google/*
echo "proto root: $ROOT"

rm -rf "$OUT"; mkdir -p "$OUT"
uvx --with grpcio --from grpcio-tools python -m grpc_tools.protoc \
  -I "$ALPHA" -I "$V2" \
  --python_out="$OUT" --grpc_python_out="$OUT" \
  "$ALPHA"/sui/rpc/v2alpha/ledger_service.proto \
  "$ALPHA"/sui/rpc/v2alpha/filter.proto \
  "$ALPHA"/sui/rpc/v2alpha/query_options.proto \
  "$V2"/sui/rpc/v2/*.proto
echo "wrote stubs -> $OUT"
