#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

if ! command -v git &> /dev/null; then
    echo "Please install git" >&2
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo "Please install cargo" >&2
    exit 1
fi

if ! command -v cargo-hakari &> /dev/null; then
    echo "Please install cargo-hakari (via cargo)" >&2
    exit 1
fi

function print_usage() {
    >&2 echo "Usage: $0 [--dry-run] [-h] -f FEATURE"
    >&2 echo
    >&2 echo "  Create a new copy of execution-related crates, and add them to the"
    >&2 echo "  workspace.  Assigning an execution layer version to the new copy and"
    >&2 echo "  implementing the Executor and Verifier traits in crates/sui-execution"
    >&2 echo "  must be done manually as a follow-up."
    >&2 echo
    >&2 echo "Options:"
    >&2 echo
    >&2 echo "  -f FEATURE        The feature to label the new cut with."
    >&2 echo "  --dry-run         Just print the operations, don't actually run them."
    >&2 echo "  -h, --help        Print this usage."

    exit "$1"
}

while getopts "f:-:h" OPT; do
    case $OPT in
        f)
            FEATURE=$OPTARG ;;
        h)
            print_usage 0 ;;
        -)  # Parsing Long Options
            case "$OPTARG" in
                dry-run)
                    DRY_RUN="--dry-run" ;;
                help)
                    print_usage 0 ;;
                *)  >&2 echo "Unrecognized option '--$OPTARG'"
                    >&2 echo
                    print_usage 1
                    ;;
            esac
            ;;
        \?)
            >&2 echo "Unrecognized option '-$OPT'"
            >&2 echo
            print_usage 1
            ;;
    esac
done

if [ -z "$FEATURE" ]; then
    >&2 echo "Error: No 'FEATURE' name given"
    >&2 echo
    print_usage 1
fi

REPO=$(git rev-parse --show-toplevel)

cd "$REPO"

>&2 echo "Cutting new release"
cargo run --bin cut --                                                  \
      $DRY_RUN --feature "$FEATURE"                                     \
      -d "sui-execution/latest:sui-execution/$FEATURE:-latest"          \
      -d "external-crates/move:external-crates/move-execution/$FEATURE" \
      -p "sui-adapter-latest"                                           \
      -p "sui-move-natives-latest"                                      \
      -p "sui-verifier-latest"                                          \
      -p "move-bytecode-verifier"                                       \
      -p "move-stdlib"                                                  \
      -p "move-vm-runtime"

if [ -z "$DRY_RUN" ]; then
    # We need to remove some special-case files/directories from the cut:
    rm -r "external-crates/move-execution/$FEATURE/move-bytecode-verifier/transactional-tests"
    rm -r "external-crates/move-execution/$FEATURE/move-stdlib/src/main.rs"
    rm -r "external-crates/move-execution/$FEATURE/move-stdlib/tests"

    cargo hakari generate
fi
