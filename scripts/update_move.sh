#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

if ! command -v git &> /dev/null; then
    echo "Please install git" >&2
    exit 1
fi

if ! command -v sed &> /dev/null; then
    echo "Please install sed" >&2
    exit 1
fi

DEFAULT_REMOTE="https://github.com/move-language/move"
DEFAULT_BRANCH="sui-move"

while getopts ":r:b:h" OPT; do
    case $OPT in
        r)
            REMOTE=$OPTARG ;;
        b)
            BRANCH=$OPTARG ;;
        h)
            >&2 echo "Usage: $0 [-h] [-r REMOTE] [-b BRANCH]"
            >&2 echo ""
            >&2 echo "Options:"
            >&2 echo " -r REMOTE          The git remote to find the move repository at."
            >&2 echo "                    (Default: $DEFAULT_REMOTE)"
            >&2 echo " -b BRANCH          The branch to depend on."
            >&2 echo "                    (Default: $DEFAULT_BRANCH)"
            exit 0
            ;;
        \?)
            >&2 echo "Unrecognized option '$OPTARG'"
            exit 1
            ;;
    esac
done

REMOTE=${REMOTE:-$DEFAULT_REMOTE}
BRANCH=${BRANCH:-$DEFAULT_BRANCH}

COMMIT=$(git ls-remote "$REMOTE" "$BRANCH" | cut -f1)
REPO=$(git rev-parse --show-toplevel)

>&2 echo "Updating Move Cargo dependency"
>&2 echo "Sui Repo    : $REPO"
>&2 echo "Move Repo   : $REMOTE"
>&2 echo "Move Branch : $BRANCH"
>&2 echo "Move Commit : $COMMIT"

>&2 echo ""
>&2 echo "Updating $REPO/Cargo.toml ..."
sed -i '' -f - "$REPO/Cargo.toml" <<EOS
/^move-/!b
s!git = "[^"]*"!git = "$REMOTE"!
s!rev = "[^"]*"!rev = "$COMMIT"!
EOS

>&2 echo "Done!"
