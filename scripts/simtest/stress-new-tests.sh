#!/bin/bash -x
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This script runs as part of the rust.yml CI workflow.
# It will detect new tests added to the repository since the last commit
# to the main branch, and run them 20 times each with a different seed.

MERGE_BASE=$(git merge-base HEAD origin/main)

# print git diff between current revision and origin/main, grep for new tests
NEW_TESTS=( $(git diff "$MERGE_BASE" \
  | grep -A1 -F '+#[sim_test]' \
  | grep -Eo '\bfn +[a-z_0-9]+\b' \
  | awk '{print $2}') )

echo "Detected new tests: ${NEW_TESTS[@]}"

# exit early if NEW_TESTS is empty
if [ -z "${NEW_TESTS}" ]; then
  echo "No new tests detected, exiting"
  exit 0
fi

# iterate over NEW_TESTS and wrap each element in 'test(=$element)'
for i in "${!NEW_TESTS[@]}"; do
  NEW_TESTS[$i]="test(=${NEW_TESTS[$i]})"
done

# join NEW_TESTS with "or"
TEST_FILTER=$(printf %s' or ' "${NEW_TESTS[@]}" | sed 's/ or *$//')

# use seed of 2, since 1 was already used by the main job
MSIM_TEST_NUM=20 MSIM_TEST_SEED=2 cargo simtest -E "$TEST_FILTER"
