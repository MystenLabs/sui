#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Unified Sui version bump script.
# Updates Cargo.toml, openrpc.json, and snap.json, then runs cargo check.
#
# This script handles file changes ONLY — git operations (commit, push, PR)
# are the caller's responsibility (workflow or operator).
#
# Usage: version-bump.sh [--type patch|minor] [--version X.Y.Z] [--non-interactive|-y]

set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────
BUMP_TYPE="patch"
OVERRIDE_VERSION=""
NON_INTERACTIVE=false

# ── Colors ────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ── Usage ─────────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Bump the Sui workspace version across all required files.

Options:
  --type patch|minor   Bump direction (default: patch)
  --version X.Y.Z      Override calculated version
  --non-interactive, -y  Skip confirmation prompt
  --help, -h           Show this help

Files updated:
  - Cargo.toml (workspace version)
  - crates/sui-open-rpc/spec/openrpc.json (API spec version)
  - crates/sui-open-rpc/tests/snapshots/generate_spec__openrpc.snap.json (snapshot)
  - Cargo.lock (regenerated via cargo check)

This script does NOT commit, push, or create PRs — the caller handles delivery.
EOF
  exit 0
}

# ── Parse flags ───────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --type)
      BUMP_TYPE="$2"
      shift 2
      ;;
    --version)
      OVERRIDE_VERSION="$2"
      shift 2
      ;;
    --non-interactive | -y)
      NON_INTERACTIVE=true
      shift
      ;;
    --help | -h)
      usage
      ;;
    *)
      echo -e "${RED}Error: unknown option '$1'${NC}" >&2
      echo "Run with --help for usage." >&2
      exit 1
      ;;
  esac
done

# Validate bump type
if [[ "$BUMP_TYPE" != "patch" && "$BUMP_TYPE" != "minor" ]]; then
  echo -e "${RED}Error: --type must be 'patch' or 'minor' (got '$BUMP_TYPE')${NC}" >&2
  exit 1
fi

# ── Extract current version ──────────────────────────────────────────
if [[ ! -f "Cargo.toml" ]]; then
  echo -e "${RED}Error: Cargo.toml not found in current directory.${NC}" >&2
  echo "Run this script from the root of the sui repository." >&2
  exit 1
fi

CURRENT_VERSION=$(sed -nE 's/^version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/p' ./Cargo.toml)
if [[ -z "$CURRENT_VERSION" ]]; then
  echo -e "${RED}Error: could not extract version from Cargo.toml.${NC}" >&2
  exit 1
fi

# ── Calculate new version ────────────────────────────────────────────
if [[ -n "$OVERRIDE_VERSION" ]]; then
  # Validate override format
  if ! [[ "$OVERRIDE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo -e "${RED}Error: --version must be in X.Y.Z format (got '$OVERRIDE_VERSION')${NC}" >&2
    exit 1
  fi
  NEW_VERSION="$OVERRIDE_VERSION"
else
  IFS=. read -r major minor patch <<<"$CURRENT_VERSION"
  if [[ "$BUMP_TYPE" == "minor" ]]; then
    NEW_VERSION="$major.$((minor + 1)).$patch"
  else
    NEW_VERSION="$major.$minor.$((patch + 1))"
  fi
fi

if [[ "$CURRENT_VERSION" == "$NEW_VERSION" ]]; then
  echo -e "${RED}Error: new version ($NEW_VERSION) is the same as current ($CURRENT_VERSION).${NC}" >&2
  exit 1
fi

# ── Confirmation ─────────────────────────────────────────────────────
echo -e "${GREEN}=== Sui Version Bump ===${NC}"
echo ""
echo "Current version: $CURRENT_VERSION"
echo -e "New version:     ${GREEN}$NEW_VERSION${NC} ($BUMP_TYPE bump)"
echo ""

if [[ "$NON_INTERACTIVE" != "true" ]]; then
  read -r -p "Proceed? (y/n): " yn
  case "$yn" in
    [Yy]*) ;;
    *)
      echo "Aborted."
      exit 0
      ;;
  esac
fi

# ── Update Cargo.toml ────────────────────────────────────────────────
echo -e "${YELLOW}Updating Cargo.toml...${NC}"
sed -i -E "s/^(version = \")[0-9]+\.[0-9]+\.[0-9]+(\"$)/\1${NEW_VERSION}\2/" Cargo.toml
echo -e "${GREEN}✓ Cargo.toml updated${NC}"

# ── Update openrpc.json ──────────────────────────────────────────────
OPENRPC_FILE="crates/sui-open-rpc/spec/openrpc.json"
if [[ -f "$OPENRPC_FILE" ]]; then
  echo -e "${YELLOW}Updating openrpc.json...${NC}"
  sed -i -E "s/(\"version\": \")([0-9]+\.[0-9]+\.[0-9]+)(\")/\1${NEW_VERSION}\3/" "$OPENRPC_FILE"
  echo -e "${GREEN}✓ openrpc.json updated${NC}"
else
  echo -e "${YELLOW}Warning: $OPENRPC_FILE not found, skipping.${NC}"
fi

# ── Update snap.json ─────────────────────────────────────────────────
SNAP_FILE="crates/sui-open-rpc/tests/snapshots/generate_spec__openrpc.snap.json"
if [[ -f "$SNAP_FILE" ]]; then
  echo -e "${YELLOW}Updating snap.json...${NC}"
  sed -i -E "s/(\"version\": \")([0-9]+\.[0-9]+\.[0-9]+)(\")/\1${NEW_VERSION}\3/" "$SNAP_FILE"
  echo -e "${GREEN}✓ snap.json updated${NC}"
else
  echo -e "${YELLOW}Warning: $SNAP_FILE not found, skipping.${NC}"
fi

# ── Cargo check ──────────────────────────────────────────────────────
echo -e "${YELLOW}Running cargo check (regenerates Cargo.lock)...${NC}"
cargo check || true
echo -e "${GREEN}✓ cargo check completed${NC}"

# ── Summary ──────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}=== Version bump complete ===${NC}"
echo "  $CURRENT_VERSION → $NEW_VERSION"
echo ""
echo "Files changed:"
echo "  - Cargo.toml"
echo "  - Cargo.lock"
[[ -f "$OPENRPC_FILE" ]] && echo "  - $OPENRPC_FILE"
[[ -f "$SNAP_FILE" ]] && echo "  - $SNAP_FILE"
echo ""
echo "NEW_VERSION=$NEW_VERSION"
