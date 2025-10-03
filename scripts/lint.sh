#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Formats and lints code. Run before committing.

set -e

echo "Formatting and linting changed files..."

CHANGED_MOVE_FILES=$(git diff --name-only remotes/origin/main | grep '\.move$' || true)

echo "Running cargo fmt..."
cargo fmt

if [ -n "$CHANGED_MOVE_FILES" ]; then
    echo "Running prettier-move on changed .move files:"
    echo "$CHANGED_MOVE_FILES"

    # Install prettier globally if not available
    if ! command -v prettier &> /dev/null; then
        echo "Installing prettier globally..."
        npm install -g prettier
    fi

    # Check if prettier-move is built, build if not
    if [ ! -f "external-crates/move/tooling/prettier-move/out/index.js" ]; then
        echo "Building prettier-move..."
        cd external-crates/move/tooling/prettier-move
        npm install --no-save
        npm run build
        cd - > /dev/null
    fi

    echo "$CHANGED_MOVE_FILES" | while read -r file; do
        if [ -f "$file" ]; then
            echo "  Formatting: $file"
            npx --prefix external-crates/move/tooling/prettier-move prettier-move -c "$file" --write
        fi
    done

    echo "Prettier-move formatting complete"
else
    echo "No .move files changed since origin/main"
fi

echo "Running cargo xclippy with warnings as errors..."
cargo xclippy -D warnings

echo "Running cargo xlint with warnings as errors..."
cargo xlint

echo "All formatting and linting complete!"