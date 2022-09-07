#!/usr/bin/env bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script requirements:
# - curl
# - jq

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Go to the repo root directory.
cd "$(git rev-parse --show-toplevel)"

# Check 1 argument is given
if [ $# -lt 1 ]
then
        echo "Usage : $0 <crate_name>"
        exit 1
fi

# The first argument should be the name of a crate.
CRATE_NAME="$1"

cargo metadata --format-version 1 | \
    jq --arg crate_name "$CRATE_NAME" --exit-status -r \
        '.packages[] | select(.name == $crate_name) | .version'
