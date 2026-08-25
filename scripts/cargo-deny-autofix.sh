#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Detects `cargo deny check advisories` errors in the CI-gated workspaces and
# attempts to fix them with semver-compatible lockfile bumps
# (`cargo update -p <pkg>@<version>`), then re-checks to verify.
#
# Must be run from the repository root. Requires cargo-deny and jq.
#
# Usage: scripts/cargo-deny-autofix.sh [workspace-dir...]
#   Defaults to the workspaces gated by CI: "." and "external-crates/move".
#
# Outputs (in $OUT_DIR, default: a fresh mktemp dir):
#   fixed.md     - one bullet per advisory fixed by a lockfile bump
#   remaining.md - one bullet per advisory that could not be auto-fixed
# When $GITHUB_OUTPUT is set, also emits: has_fixes, has_remaining, out_dir.

set -euo pipefail

WORKSPACES=("$@")
if [[ ${#WORKSPACES[@]} -eq 0 ]]; then
  WORKSPACES=("." "external-crates/move")
fi

# Standalone example workspaces: not gated by cargo-deny in CI, but keep their
# lockfiles in sync when we bump a crate (best effort).
EXTRA_LOCKFILE_DIRS=(
  examples/rust/basic-sui-indexer
  examples/rust/clickhouse-sui-indexer
  examples/rust/walrus-attributes-indexer
)

OUT_DIR="${OUT_DIR:-$(mktemp -d)}"
mkdir -p "$OUT_DIR"
FIXED_MD="$OUT_DIR/fixed.md"
REMAINING_MD="$OUT_DIR/remaining.md"
: > "$FIXED_MD"
: > "$REMAINING_MD"

# Scans one workspace, populating ADVISORIES with one JSON object per
# error-level advisory occurrence:
# {"id": "RUSTSEC-...", "pkg": "<crate>", "ver": "<locked version>"}
# Any cargo-deny failure that is not an advisory diagnostic (config error,
# advisory-DB fetch failure, unparseable output) aborts the script so it can't
# be mistaken for a clean result.
ADVISORIES=()
scan() {
  local ws="$1" diag json deny_status=0
  diag="$(mktemp)"
  json="$(mktemp)"
  # cargo-deny writes JSON diagnostics to stderr and exits non-zero on errors.
  cargo deny --manifest-path "$ws/Cargo.toml" --format json check advisories 2> "$diag" || deny_status=$?

  # Other tooling can interleave non-JSON noise on the same stderr stream,
  # e.g. rustup printing "info: syncing channel updates" while auto-installing
  # the pinned toolchain on a fresh runner. cargo-deny emits one JSON object
  # per line, so parse only lines that look like JSON and log the rest.
  grep '^{' "$diag" > "$json" || true
  grep -v '^{' "$diag" | sed 's/^/    [cargo-deny stderr] /' || true

  if ! jq -es '.' "$json" > /dev/null 2>&1; then
    echo "::error::cargo-deny output for '$ws' is not valid JSON (exit ${deny_status}); raw output:"
    cat "$diag"
    exit 1
  fi

  # cargo-deny ends every completed check with a summary diagnostic; if it is
  # missing, the output was truncated or cargo-deny died mid-run.
  if ! jq -es 'any(.[]; .type == "summary")' "$json" > /dev/null 2>&1; then
    echo "::error::cargo-deny output for '$ws' has no summary diagnostic (exit ${deny_status}); raw output:"
    cat "$diag"
    exit 1
  fi

  local other
  other=$(jq -c 'select(.type == "diagnostic" and .fields.severity == "error" and .fields.advisory == null)' "$json")
  if [[ -n "$other" ]]; then
    echo "::error::cargo-deny reported non-advisory errors for '$ws':"
    echo "$other"
    exit 1
  fi

  mapfile -t ADVISORIES < <(jq -c '
    select(.type == "diagnostic" and .fields.severity == "error" and .fields.advisory != null)
    | {id: .fields.advisory.id} + (.fields.graphs[].Krate | {pkg: .name, ver: .version})
  ' "$json" | sort -u)

  if [[ $deny_status -ne 0 && ${#ADVISORIES[@]} -eq 0 ]]; then
    echo "::error::cargo deny check advisories failed for '$ws' with no advisory diagnostics (exit ${deny_status}); raw output:"
    cat "$diag"
    exit 1
  fi
  rm -f "$diag" "$json"
}

declare -A bumped_pkgs=()

for ws in "${WORKSPACES[@]}"; do
  echo "==> Checking advisories in '$ws'"
  scan "$ws"
  found=("${ADVISORIES[@]}")
  if [[ ${#found[@]} -eq 0 ]]; then
    echo "    No advisory errors."
    continue
  fi

  # Attempt a semver-compatible bump for each vulnerable pkg@version.
  declare -A new_ver=()
  for adv in "${found[@]}"; do
    id=$(jq -r '.id' <<< "$adv")
    pkg=$(jq -r '.pkg' <<< "$adv")
    ver=$(jq -r '.ver' <<< "$adv")
    echo "    $id: $pkg@$ver"
    update_log=$(cargo update --manifest-path "$ws/Cargo.toml" -p "${pkg}@${ver}" 2>&1) || true
    echo "      ${update_log//$'\n'/$'\n'      }"
    # cargo prints "Updating <pkg> v<old> -> v<new>" when the bump succeeds.
    bumped=$(sed -n "s/^ *Updating ${pkg} v${ver} -> v\([^ ]*\).*/\1/p" <<< "$update_log")
    if [[ -n "$bumped" ]]; then
      new_ver["${id}|${pkg}|${ver}"]="$bumped"
      bumped_pkgs["$pkg"]=1
    fi
  done

  # Re-check: anything still erroring could not be fixed by a compatible bump.
  scan "$ws"
  still=("${ADVISORIES[@]}")
  declare -A still_keys=()
  for adv in "${still[@]}"; do
    id=$(jq -r '.id' <<< "$adv")
    pkg=$(jq -r '.pkg' <<< "$adv")
    ver=$(jq -r '.ver' <<< "$adv")
    still_keys["${id}|${pkg}|${ver}"]=1
    echo "- [${id}](https://rustsec.org/advisories/${id}): \`${pkg}\` ${ver} in \`${ws}\` (no semver-compatible fix; needs a manual bump or a \`deny.toml\` ignore)" >> "$REMAINING_MD"
  done
  for adv in "${found[@]}"; do
    id=$(jq -r '.id' <<< "$adv")
    pkg=$(jq -r '.pkg' <<< "$adv")
    ver=$(jq -r '.ver' <<< "$adv")
    key="${id}|${pkg}|${ver}"
    if [[ -z "${still_keys[$key]:-}" && -n "${new_ver[$key]:-}" ]]; then
      echo "- [${id}](https://rustsec.org/advisories/${id}): \`${pkg}\` ${ver} → ${new_ver[$key]} in \`${ws}\`" >> "$FIXED_MD"
    fi
  done
  unset new_ver still_keys
done

# Propagate bumps to the standalone example lockfiles so they don't drift.
if [[ ${#bumped_pkgs[@]} -gt 0 ]]; then
  for dir in "${EXTRA_LOCKFILE_DIRS[@]}"; do
    [[ -f "$dir/Cargo.lock" ]] || continue
    for pkg in "${!bumped_pkgs[@]}"; do
      cargo update --manifest-path "$dir/Cargo.toml" -p "$pkg" 2>/dev/null || true
    done
  done
fi

has_fixes=false
[[ -s "$FIXED_MD" ]] && has_fixes=true
has_remaining=false
[[ -s "$REMAINING_MD" ]] && has_remaining=true

echo "==> Summary"
echo "    Fixed:"
sed 's/^/      /' "$FIXED_MD"
echo "    Remaining (need manual attention):"
sed 's/^/      /' "$REMAINING_MD"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "has_fixes=$has_fixes"
    echo "has_remaining=$has_remaining"
    echo "out_dir=$OUT_DIR"
  } >> "$GITHUB_OUTPUT"
fi
