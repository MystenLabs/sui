#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# One Sui version must map to one released commit. release.yml reuses release
# archives by version, so devnet/testnet/mainnet tags that share a version but
# point at different commits ship the wrong binaries for one of them.
#
# Usage: check-release-version.sh <version> [<commit>]
#   without <commit>: fail if any {devnet,testnet,mainnet}-v<version> tag exists
#   with <commit>:    fail if such a tag points at a commit other than <commit>
# Exit codes: 0 OK, 1 version already released elsewhere, 2 lookup failed.

set -euo pipefail

version=${1:-}
commit=${2:-}
remote=${SUI_REMOTE:-https://github.com/MystenLabs/sui.git}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "invalid version '$version' (expected X.Y.Z)"
  exit 2
fi
if [[ -n $commit && ! $commit =~ ^[0-9a-f]{40}$ ]]; then
  echo "invalid commit '$commit' (expected a full 40-character SHA)"
  exit 2
fi

if ! refs=$(git ls-remote --tags "$remote"); then
  echo "could not list tags of $remote"
  exit 2
fi

# Annotated tags list a peeled "^{}" entry with the commit; prefer it.
tags=$(awk -v v="$version" '{
  t = $2; sub(/^refs\/tags\//, "", t); peeled = sub(/\^\{\}$/, "", t)
  i = index(t, "-v"); if (!i) next
  n = substr(t, 1, i - 1)
  if ((n != "devnet" && n != "testnet" && n != "mainnet") || substr(t, i + 2) != v) next
  if (peeled || !(t in sha)) sha[t] = $1
} END { for (t in sha) print t, sha[t] }' <<<"$refs" | sort)

conflicts=""
while read -r tag sha; do
  [[ -z $tag ]] && continue
  if [[ -z $commit || $sha != "$commit" ]]; then
    conflicts+="  $tag -> $sha"$'\n'
  fi
done <<<"$tags"

if [[ -n $conflicts ]]; then
  if [[ -n $commit ]]; then
    echo "Version $version is already released from a different commit than $commit:"
  else
    echo "Version $version is already released:"
  fi
  printf '%s' "$conflicts"
  echo "Bump the patch version (scripts/version-bump.sh --type patch) so every released commit has its own version."
  exit 1
fi
echo "OK: no release tag uses version $version${commit:+ on a commit other than $commit}"
