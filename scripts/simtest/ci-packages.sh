#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Prints a nextest filter that selects the packages whose tests run in the CI simtest job. CI
# builds the whole workspace under msim and uses this filter to choose which tests run.
#   (no argument)  every package below
#   --mainnet      the packages whose simulator tests depend on chain-specific protocol config
#   --check        fails if the list of packages with simulator tests is out of date

set -euo pipefail

# Packages that contain simulator tests.
SIM_TEST_PACKAGES=(
  sui-e2e-tests
  sui-core
  sui
  sui-benchmark
  consensus-simtests
  sui-indexer-alt-e2e-tests
  sui-single-node-benchmark
  sui-network
  sui-sdk
  test-cluster
  sui-surfer
  sui-storage
)

# Packages without simulator tests whose plain tests cover msim-only code paths.
MSIM_UNIT_TEST_PACKAGES=(
  consensus-core
  mysten-common
  sui-framework-tests
)

MAINNET_PACKAGES=(
  sui-e2e-tests
  sui-core
  sui-benchmark
  sui-sdk
  sui-single-node-benchmark
  sui
  sui-indexer-alt-e2e-tests
)

filter_for() {
  local filter=""
  for p in "$@"; do
    filter+="${filter:+ | }package($p)"
  done
  echo "$filter"
}

case "${1:-}" in
  "")
    filter_for "${SIM_TEST_PACKAGES[@]}" "${MSIM_UNIT_TEST_PACKAGES[@]}"
    exit 0
    ;;
  --mainnet)
    filter_for "${MAINNET_PACKAGES[@]}"
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

# Capture the scan before comparing so that a failed scan fails the check.
files="$(git grep -lE "$SIM_TEST_ATTR" -- '*.rs' ':!crates/sui-proc-macros' ':!external-crates')"
found=()
while read -r dir; do
  found+=("$(package_of "$dir/x")")
done < <(echo "$files" | xargs -n1 dirname | sort -u)
found_list=" $(printf '%s\n' "${found[@]}" | sort -u | tr '\n' ' ') "

status=0
for pkg in $found_list; do
  if [[ ! " ${SIM_TEST_PACKAGES[*]} " =~ " ${pkg} " ]]; then
    echo "package $pkg contains simulator tests but is not in SIM_TEST_PACKAGES in $0" >&2
    status=1
  fi
done
for pkg in "${SIM_TEST_PACKAGES[@]}"; do
  if [[ ! "$found_list" =~ " ${pkg} " ]]; then
    echo "package $pkg is in SIM_TEST_PACKAGES in $0 but contains no simulator tests" >&2
    status=1
  fi
done
exit $status
