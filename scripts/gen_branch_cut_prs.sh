#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# This script creates two PRs:
# 1. Generate framework bytecode snapshot PR
#   cargo run --bin sui-framework-snapshot
# 2. Generate a version bump PR

set -euo pipefail

# check required params
if [[ $# -ne 1 ]]; then
  echo "USAGE: gen_branch_cut_prs.sh <snapshot|version-bump>"
  exit 1
fi
PR_TYPE=$1

# Ensure required binaries are available
for cmd in gh git cargo; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Error: required command '$cmd' not found in PATH." >&2
    exit 1
  fi
done

# Ensure gh is authenticated (in CI this uses GH_TOKEN/GITHUB_TOKEN)
if ! gh auth status >/dev/null 2>&1; then
  if [[ -n "${GITHUB_TOKEN:-}" && -z "${GH_TOKEN:-}" ]]; then
    export GH_TOKEN="$GITHUB_TOKEN"
  fi
  if ! gh auth status >/dev/null 2>&1; then
    echo "Error: gh is not authenticated. Set GH_TOKEN/GITHUB_TOKEN in the environment with pull-requests:write." >&2
    exit 1
  fi
fi

# Make sure GITHUB_ACTOR is set.
if [[ -z "${GITHUB_ACTOR:-}" ]]; then
  GITHUB_ACTOR="$(whoami 2>/dev/null || echo github-actions[bot])"
fi

# Configure git user
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

# Get current main version
SUI_VERSION=$(sed -nE 's/^version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/p' ./Cargo.toml)
STAMP="$(date +%Y%m%d%H%M%S)"

if [[ "$PR_TYPE" == *snapshot* ]]; then
  echo "Generating framework bytecode snapshot..."
  # Set up branch for changes.
  BRANCH="${GITHUB_ACTOR}/sui-v${SUI_VERSION}-bytecode-framework-snapshot-${STAMP}"
  git checkout -b "$BRANCH"

  # Generate framework bytecode snapshot
  cargo run --bin sui-framework-snapshot

  # Staged all changes
  echo "Staging all changed files..."
  git add -A .

  # Generate PR body
  BODY="Sui v${SUI_VERSION} Framework Bytecode snapshot"

  # Commit, push, and create PR.
  git commit -m "$BODY"
  git push -u origin "$BRANCH"

  # Create PR with proper error handling
  echo "Creating pull request..."
  if PR_OUTPUT=$(gh pr create \
    --base main \
    --head "$BRANCH" \
    --title "$BODY" \
    --reviewer "ebmifa,pei-mysten,tharbert" \
    --body "$BODY" 2>&1); then
    
    # Extract PR URL from output
    if PR_URL=$(echo "$PR_OUTPUT" | grep -Eo 'https://github.com/[^ ]+'); then
      echo "Successfully created PR: $PR_URL"
    else
      echo "Warning: PR created but could not extract URL from output:"
      echo "$PR_OUTPUT"
      PR_URL="(URL extraction failed)"
    fi
  else
    echo "Error: Failed to create pull request:" >&2
    echo "$PR_OUTPUT" >&2
    exit 1
  fi

  # Setting the PR to auto merge
  gh pr merge --auto --squash --delete-branch "$BRANCH"
  echo "Pull request for Sui v${SUI_VERSION} Framework Bytecode snapshot created: $PR_URL"

elif [[ "$PR_TYPE" == *version-bump* ]]; then
  # Generate the version bump PR
  echo "Generating version bump..."
  # Bump main branhch version
  IFS=. read -r major minor patch <<<"$SUI_VERSION"; NEW_SUI_VERSION="$major.$((minor+1)).$patch"

  # Setup new branch for staging
  BRANCH="${GITHUB_ACTOR}/sui-v${NEW_SUI_VERSION}-version-bump-${STAMP}"
  git checkout -b "$BRANCH"

  # Update the version in Cargo.toml and openrpc.json
  sed -i -E "s/^(version = \")[0-9]+\.[0-9]+\.[0-9]+(\"$)/\1${NEW_SUI_VERSION}\2/" Cargo.toml
  sed -i -E "s/(\"version\": \")([0-9]+\.[0-9]+\.[0-9]+)(\")/\1${NEW_SUI_VERSION}\3/" crates/sui-open-rpc/spec/openrpc.json

  # Cargo check to generate Cargo.lock changes
  cargo check || true

  # Staged all changes
  echo "Staging all changed files..."
  git add -A .

  # Generate PR body
  BODY="Sui v${NEW_SUI_VERSION} Version Bump"

  git commit -m "$BODY"
  git push -u origin "$BRANCH"

  # Create PR with proper error handling
  echo "Creating pull request..."
  if PR_OUTPUT=$(gh pr create \
    --base main \
    --head "$BRANCH" \
    --title "$BODY" \
    --reviewer "ebmifa,pei-mysten,tharbert" \
    --body "$BODY" 2>&1); then
    
    # Extract PR URL from output
    if PR_URL=$(echo "$PR_OUTPUT" | grep -Eo 'https://github.com/[^ ]+'); then
      echo "Successfully created PR: $PR_URL"
    else
      echo "Warning: PR created but could not extract URL from output:"
      echo "$PR_OUTPUT"
      PR_URL="(URL extraction failed)"
    fi
  else
    echo "Error: Failed to create pull request:" >&2
    echo "$PR_OUTPUT" >&2
    exit 1
  fi

  echo "Pull request for Sui v${NEW_SUI_VERSION} Version Bump created: $PR_URL"
else
  echo "Invalid argument. Use 'snapshot' or 'version-bump'."
  exit 1
fi
