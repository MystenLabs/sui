#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Prints a nextest filter that selects the packages whose simulator tests run in CI. CI builds
# the whole workspace and uses this filter to choose which tests run.
# With `--check`, instead fails if a package that contains simulator tests is missing from the
# list.

set -euo pipefail

PACKAGES=(
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

if [[ "${1:-}" != "--check" ]]; then
  filter=""
  for p in "${PACKAGES[@]}"; do
    filter+="${filter:+ | }package($p)"
  done
  echo "$filter"
  exit 0
fi

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

missing=0
while read -r pkg; do
  if [[ ! " ${PACKAGES[*]} " =~ " ${pkg} " ]]; then
    echo "package $pkg contains simulator tests but is not in $0" >&2
    missing=1
  fi
done < <(
  git grep -lE '#\[(sui_macros::|sui_simulator::)?(sim_test|sui_test)[](]|#\[msim::test[](]' \
    -- '*.rs' ':!crates/sui-proc-macros' ':!external-crates' |
    while read -r f; do package_of "$f"; done | sort -u
)
exit $missing
