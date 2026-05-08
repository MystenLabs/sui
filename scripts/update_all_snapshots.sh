#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Automatically update all snapshots. This is needed when the framework is changed or when protocol
# config is changed.
# By default, this script will update all affected snapshot files. If you want the `.snap` files to
# remain unchanged, set the environment variable `INSTA_UPDATE="new"` before running this script.
# The `.snap.new` files can then bew reviewed using `cargo insta review`.

read -r -d '' USAGE <<'EOF'
Automatically update all snapshot files when the framework or protocol configuration changes.

By default, this script updates all affected `.snap` files in place, by using`INSTA_UPDATE="always"`.
If you want existing `.snap` files to remain unchanged,
set the environment variable, e.g. INSTA_UPDATE="new", which will create new `.snap.new` files instead of overwriting existing `.snap` files. You can then review the changes using `cargo insta review`.
For more information on using `INSTA_UPDATE`, see https://docs.rs/insta/latest/insta/#updating-snapshots.

Examples:
    # Updates all snapshots in place.
    ./update_all_snapshots.sh

    # Creates `.snap.new` files and keeps original snapshots unchanged and reviews them afterwards.
    INSTA_UPDATE="new" ./update_all_snapshots.sh
    cargo insta review
EOF
if [[ $# -gt 0 ]]; then
    echo "$USAGE"
    exit 0
fi

set -x
set -e

SCRIPT_PATH=$(realpath "$0")
SCRIPT_DIR=$(dirname "$SCRIPT_PATH")
ROOT="$SCRIPT_DIR/.."
# Check if INSTA_UPDATE is set; if not, set it to "always"
if [ -z "$INSTA_UPDATE" ]; then
    INSTA_UPDATE="always"
    export INSTA_UPDATE
fi

# This technically should be pulling from `.config/insta.yaml`, but we set the test runner again
# here to be safe and explicit.
INSTA=(cargo insta test --test-runner nextest --no-test-runner-fallback)

cd "$ROOT"
UPDATE=1 cargo nextest run -p sui-framework --test build-system-packages
"${INSTA[@]}" \
    -p sui-protocol-config \
    -p sui-swarm-config \
    -p sui-open-rpc \
    -p sui-types
"${INSTA[@]}" -p sui-core -- snapshot_tests
"${INSTA[@]}" -p sui-indexer-alt-graphql -- test_schema_sdl_export
"${INSTA[@]}" --features staging -p sui-indexer-alt-graphql -- test_schema_sdl_export
exit 0
