#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# This script creates two PRs:
# 1. Generate framework bytecode snapshot PR
#   cargo run --bin sui-framework-snapshot
# 2. Generate a version bump PR

set -Eeuo pipefail

# Ensure required binaries are available
for cmd in gh git; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Error: required command '$cmd' not found in PATH." >&2
    exit 1
  fi
done

# Make sure GITHUB_ACTOR is set.
if [[ -z "${GITHUB_ACTOR:-}" ]]; then
  GITHUB_ACTOR="$(whoami 2>/dev/null || echo github-actions[bot])"
fi

# Get current main version
SUI_VERSION=$(sed -nE 's/^version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/p' ./Cargo.toml)

# Set up branch for changes.
STAMP="$(date +%Y%m%d%H%M%S)"
BRANCH="${GITHUB_ACTOR}/sui-v${SUI_VERSION}-bytecode-framework-snapshot-${STAMP}"
git checkout -b "$BRANCH"

# Generate framework bytecode snapshot
cargo run --bin sui-framework-snapshot

# Staged all changes
echo "Staging all changed files..."
git add -A .

# Commit, push, and create PR.
git config user.name "github-actions[bot]"
git config user.email \
  "41898282+github-actions[bot]@users.noreply.github.com"

git commit -m "Sui v${SUI_VERSION} Framework Bytecode snapshot"
git push -u origin "$BRANCH"

# Generate PR body
BODY=$(cat <<-EOF
Sui v${SUI_VERSION} Framework Bytecode snapshot
EOF
)

PR_URL=$(gh pr create \
  --base main \
  --head "$BRANCH" \
  --title "Sui v${SUI_VERSION} Framework Bytecode snapshot" \
  --reviewer "MystenLabs/mysten-pe" \
  --body "$BODY" \
  2>&1 | grep -Eo 'https://github.com/[^ ]+')

echo "Pull request for Sui v${SUI_VERSION} Framework Bytecode snapshot created: $PR_URL"

# Setting the PR to auto merge
gh pr merge --auto --squash --delete-branch "$BRANCH"

# Generate the version bump PR
# Bump main branhch version
IFS=. read -r major minor patch <<<"$SUI_VERSION"; NEW_SUI_VERSION="$major.$((minor+1)).$patch"

# Setup new branch for staging
BRANCH="${GITHUB_ACTOR}/sui-v${NEW_SUI_VERSION}-version-bump-${STAMP}"
git checkout main && git pull origin main
git checkout -b "$BRANCH"

# Update the version in Cargo.toml and openrpc.json
sed -i -E "s/^(version = \")[0-9]+\.[0-9]+\.[0-9]+(\"$)/\1${NEW_SUI_VERSION}\2/" Cargo.toml
sed -i -E "s/(\"version\": \")([0-9]+\.[0-9]+\.[0-9]+)(\")/\1${NEW_SUI_VERSION}\3/" crates/sui-open-rpc/spec/openrpc.json

# Cargo check to generate Cargo.lock changes
cargo check || true

# Staged all changes
echo "Staging all changed files..."
git add -A .

# Commit, push, and create PR.
git config user.name "github-actions[bot]"
git config user.email \
  "41898282+github-actions[bot]@users.noreply.github.com"

git commit -m "Sui v${NEW_SUI_VERSION} Version Bump"
git push -u origin "$BRANCH"

# Generate PR body
BODY=$(cat <<-EOF
Sui v${NEW_SUI_VERSION} Version Bump
EOF
)

PR_URL=$(gh pr create \
  --base main \
  --head "$BRANCH" \
  --title "Sui v${NEW_SUI_VERSION} Version Bump" \
  --reviewer "MystenLabs/mysten-pe" \
  --body "$BODY" \
  2>&1 | grep -Eo 'https://github.com/[^ ]+')

echo "Pull request for Sui v${NEW_SUI_VERSION} Version Bump created: $PR_URL"
