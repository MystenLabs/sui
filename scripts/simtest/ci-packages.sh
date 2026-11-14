#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Prints a nextest filter that selects the packages whose tests run in the CI simtest job. CI
# builds the whole workspace under msim and uses this filter to choose which tests run. Every
# workspace package runs unless it is excluded below, so a new package runs by default.
#   (no argument)  every package that is not excluded
#   --mainnet      the same set without MAINNET_EXCLUDED_PACKAGES
#   --check        fails if an excluded package contains simulator tests or is not a
#                  workspace package

set -euo pipefail

# Packages without simulator tests whose plain tests do not cover msim-only code. Their tests
# run in the test job.
EXCLUDED_PACKAGES=(
  anemo-benchmark
  bin-version
  consensus-config
  consensus-types
  mysten-metrics
  mysten-network
  mysten-service
  prometheus-closure-metric
  shared-crypto
  simulacrum
  sui-adapter-latest
  sui-adapter-transactional-tests
  sui-adapter-v0
  sui-adapter-v1
  sui-adapter-v2
  sui-adapter-v3
  sui-adapter-v4
  sui-analytics-indexer
  sui-analytics-indexer-derive
  sui-authority-aggregation
  sui-bridge
  sui-bridge-cli
  sui-bridge-indexer
  sui-bridge-indexer-alt
  sui-bridge-schema
  sui-checkpoint-blob-indexer
  sui-cluster-test
  sui-config
  sui-consistent-store
  sui-cost
  sui-data-ingestion-core
  sui-data-store
  sui-display
  sui-enum-compat-util
  sui-execution
  sui-execution-backtest
  sui-execution-cut
  sui-faucet
  sui-field-count
  sui-field-count-derive
  sui-fork
  sui-fork-e2e-tests
  sui-framework
  sui-framework-snapshot
  sui-futures
  sui-genesis-builder
  sui-indexer-alt
  sui-indexer-alt-consistent-api
  sui-indexer-alt-consistent-store
  sui-indexer-alt-framework
  sui-indexer-alt-framework-store-traits
  sui-indexer-alt-graphql
  sui-indexer-alt-metrics
  sui-indexer-alt-object-store
  sui-indexer-alt-reader
  sui-indexer-alt-schema
  sui-inverted-index
  sui-json
  sui-json-rpc-types
  sui-keys
  sui-kv-rpc
  sui-kvstore
  sui-light-client
  sui-macros
  sui-metric-checker
  sui-metrics-push-client
  sui-move
  sui-move-build
  sui-move-lsp
  sui-move-natives-latest
  sui-move-natives-v0
  sui-move-natives-v1
  sui-move-natives-v2
  sui-move-natives-v3
  sui-move-natives-v4
  sui-name-service
  sui-node
  sui-oracle
  sui-package-alt
  sui-package-dump
  sui-package-management
  sui-package-resolver
  sui-pg-db
  sui-proc-macros
  sui-prompt
  sui-protocol-config
  sui-protocol-config-macros
  sui-proxy
  sui-replay-2
  sui-rosetta
  sui-rpc-api
  sui-rpc-benchmark
  sui-rpc-resolver
  sui-rpc-store
  sui-simulator
  sui-snapshot
  sui-source-verification
  sui-sql-macro
  sui-swarm
  sui-swarm-config
  sui-synthetic-ingestion
  sui-telemetry
  sui-test-transaction-builder
  sui-test-validator
  sui-tls
  sui-tool
  sui-transaction-builder
  sui-transaction-checks
  sui-transactional-test-runner
  sui-types
  sui-upgrade-compatibility-transactional-tests
  sui-verifier-latest
  sui-verifier-transactional-tests
  sui-verifier-v0
  sui-verifier-v1
  sui-verifier-v2
  sui-verifier-v3
  sui-verifier-v4
  telemetry-subscribers
  transaction-fuzzer
  typed-store
  typed-store-derive
  typed-store-error
  typed-store-workspace-hack
  x
)

# Packages whose simulator tests do not depend on chain-specific protocol config.
MAINNET_EXCLUDED_PACKAGES=(
  consensus-core
  consensus-simtests
  mysten-common
  sui-framework-tests
  sui-network
  sui-storage
  sui-surfer
  test-cluster
)

filter_excluding() {
  local filter=""
  for p in "$@"; do
    filter+="${filter:+ | }package($p)"
  done
  echo "all() - ($filter)"
}

case "${1:-}" in
  "")
    filter_excluding "${EXCLUDED_PACKAGES[@]}"
    exit 0
    ;;
  --mainnet)
    filter_excluding "${EXCLUDED_PACKAGES[@]}" "${MAINNET_EXCLUDED_PACKAGES[@]}"
    exit 0
    ;;
  --check) ;;
  *)
    echo "usage: $0 [--mainnet | --check]" >&2
    exit 2
    ;;
esac

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Name of the package that owns a source file: the nearest Cargo.toml above it.
package_of() {
  local dir
  dir="$(dirname "$1")"
  while [[ ! -f "$dir/Cargo.toml" ]]; do
    dir="$(dirname "$dir")"
  done
  sed -n 's/^name *= *"\(.*\)"/\1/p' "$dir/Cargo.toml" | head -1
}

# Matches #[sim_test], #[sui_test] and their path-qualified and cfg_attr forms.
SIM_TEST_ATTR='(#\[|cfg_attr\([^,]*, *)(::)?((sui_macros|sui_simulator|msim)::)?(sim_test|sui_test)[](]'

# Capture each scan before comparing so that a failed scan fails the check.
files="$(git grep -lE "$SIM_TEST_ATTR" -- '*.rs' ':!crates/sui-proc-macros' ':!external-crates')"
found=()
while read -r dir; do
  found+=("$(package_of "$dir/x")")
done < <(echo "$files" | xargs -n1 dirname | sort -u)
found_list=" $(printf '%s\n' "${found[@]}" | sort -u | tr '\n' ' ') "

metadata="$(cargo metadata --no-deps --format-version 1)"
workspace_list=" $(echo "$metadata" | python3 -c 'import json,sys; print(" ".join(p["name"] for p in json.load(sys.stdin)["packages"]))') "

status=0
for pkg in "${EXCLUDED_PACKAGES[@]}"; do
  if [[ "$found_list" =~ " ${pkg} " ]]; then
    echo "package $pkg contains simulator tests but is in EXCLUDED_PACKAGES in $0" >&2
    status=1
  fi
done
for pkg in "${EXCLUDED_PACKAGES[@]}" "${MAINNET_EXCLUDED_PACKAGES[@]}"; do
  if [[ ! "$workspace_list" =~ " ${pkg} " ]]; then
    echo "package $pkg is listed in $0 but is not a workspace package" >&2
    status=1
  fi
done
exit $status
