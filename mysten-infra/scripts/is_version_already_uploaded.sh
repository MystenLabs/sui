#!/usr/bin/env bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Script requirements:
# - curl
# - jq
# - sort with `-V` flag, available in `coreutils-7`
#   On macOS this may require `brew install coreutils`.

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# source directory
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Go to the repo root directory.
cd "$(git rev-parse --show-toplevel)"

# Check 1 arguments is given
if [ $# -lt 1 ]
then
        echo "Usage : $0 <crate_name>"
        exit 1
fi


# The first argument should be the name of a crate.
CRATE_NAME="$1"

CURRENT_VERSION="$("$DIR"/get_current_version.sh "$CRATE_NAME")" || \
    (echo >&2 "No crate named $CRATE_NAME found in workspace."; exit 1)
echo >&2 "Crate $CRATE_NAME current version: $CURRENT_VERSION"

# The leading whitespace is important! With it, we know that every version is both
# preceded by and followed by whitespace. We use this fact to avoid matching
# on substrings of versions.
EXISTING_VERSIONS="
$( \
    curl 2>/dev/null "https://crates.io/api/v1/crates/$CRATE_NAME" | \
    jq --exit-status -r 'try .versions[].num' \
)"
echo >&2 -e "Versions on crates.io:$EXISTING_VERSIONS\n"

# Use version sort (sort -V) to get all versions in ascending order, then use grep to:
# - grab the first line that matches the current version (--max-count=1)
# - only match full lines (--line-regexp)
OUTPUT="$( \
    echo -e "$EXISTING_VERSIONS" | \
    sort -V | \
    grep --line-regexp --max-count=1 "$CURRENT_VERSION" || true
)"

if [[ "$OUTPUT" == "$CURRENT_VERSION" ]]; then
    echo >&2 "The current version $CURRENT_VERSION is already on crates.io"
    exit 7
fi
