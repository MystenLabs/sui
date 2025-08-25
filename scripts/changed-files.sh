#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

# Check for modified or untracked files after CI has run
diff="$(git diff)"
echo "${diff}"
[[ -z "${diff}" ]]

changed_files="$(git status --porcelain)"
echo "${changed_files}"
[[ -z "${changed_files}" ]]
