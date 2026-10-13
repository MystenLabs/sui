#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Detects `cargo deny check advisories` errors in the CI-gated workspaces and
# attempts to fix them with semver-compatible lockfile bumps
# (`cargo update -p <pkg>@<version>`), then re-checks to verify.
#
# Must be run from the repository root. Requires cargo-deny, jq and curl.
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

# Prints the newest non-yanked, non-prerelease release of crate $1 that is
# semver-compatible with version $2, read from the crates.io sparse index.
latest_compatible() {
  local name="${1,,}" path
  case ${#name} in
    1) path="1/$name" ;;
    2) path="2/$name" ;;
    3) path="3/${name:0:1}/$name" ;;
    *) path="${name:0:2}/${name:2:2}/$name" ;;
  esac
  curl -fsSL "https://index.crates.io/$path" 2> /dev/null | jq -rs --arg v "$2" '
    def parts: split(".") | map(tonumber);
    def compat: if .[0] > 0 then [.[0]] elif .[1] > 0 then [0, .[1]] else . end;
    ($v | parts | compat) as $c
    | [.[] | select(.yanked | not) | .vers | select(test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))
       | select(parts | compat == $c)]
    | max_by(parts) // empty
  ' || true
}

# Bumps $2@$3 in workspace $1 to a newer semver-compatible version, leaving
# cargo's output in UPDATE_LOG. A plain `cargo update -p` changes nothing when
# the newer release needs a newer version of a crate that is already locked
# (e.g. quinn-proto 0.11.17 needs rand >= 0.10.1 while rand 0.10.0 is locked),
# so on no-op ask cargo for the latest compatible version with --precise, read
# the blocking crate off the resolver error and unlock it alongside. This
# touches only the crates in the conflict chain, unlike `--recursive`.
bump() {
  local ws="$1" pkg="$2" ver="$3" target log conflict
  local args=(-p "${pkg}@${ver}")
  UPDATE_LOG=$(cargo update --manifest-path "$ws/Cargo.toml" "${args[@]}" 2>&1) || true
  grep -q "Updating ${pkg} v${ver} -> " <<< "$UPDATE_LOG" && return
  target=$(latest_compatible "$pkg" "$ver")
  [[ -n "$target" && "$target" != "$ver" ]] || return 0
  for _ in 1 2 3 4 5; do
    log=$(cargo update --manifest-path "$ws/Cargo.toml" -p "${pkg}@${ver}" --precise "$target" 2>&1) && {
      UPDATE_LOG="$log"
      return
    }
    # shellcheck disable=SC2016 # literal backticks from cargo's message
    conflict=$(sed -n 's/.*previously selected package `\([^ ]*\) v\([^`]*\)`.*/\1@\2/p' <<< "$log" | head -n1)
    [[ -n "$conflict" ]] || return 0
    args+=(-p "$conflict")
    log=$(cargo update --manifest-path "$ws/Cargo.toml" "${args[@]}" 2>&1) || true
    if grep -q "Updating ${pkg} v${ver} -> " <<< "$log"; then
      UPDATE_LOG="$log"
      return
    fi
  done
}

for ws in "${WORKSPACES[@]}"; do
  echo "==> Checking advisories in '$ws'"
  scan "$ws"
  found=("${ADVISORIES[@]}")
  if [[ ${#found[@]} -eq 0 ]]; then
    echo "    No advisory errors."
    continue
  fi

  # Attempt a semver-compatible bump for each vulnerable pkg@version.
  declare -A new_ver=() new_ver_by_pkg=()
  for adv in "${found[@]}"; do
    id=$(jq -r '.id' <<< "$adv")
    pkg=$(jq -r '.pkg' <<< "$adv")
    ver=$(jq -r '.ver' <<< "$adv")
    echo "    $id: $pkg@$ver"
    # Several advisories can share a pkg@version; bump it only once.
    if [[ -n "${new_ver_by_pkg["${pkg}@${ver}"]:-}" ]]; then
      new_ver["${id}|${pkg}|${ver}"]="${new_ver_by_pkg["${pkg}@${ver}"]}"
      echo "      already bumped to ${new_ver_by_pkg["${pkg}@${ver}"]}"
      continue
    fi
    bump "$ws" "$pkg" "$ver"
    echo "      ${UPDATE_LOG//$'\n'/$'\n'      }"
    # cargo prints "Updating <pkg> v<old> -> v<new>" when the bump succeeds.
    bumped=$(sed -n "s/^ *Updating ${pkg} v${ver} -> v\([^ ]*\).*/\1/p" <<< "$UPDATE_LOG")
    if [[ -n "$bumped" ]]; then
      new_ver["${id}|${pkg}|${ver}"]="$bumped"
      new_ver_by_pkg["${pkg}@${ver}"]="$bumped"
      while read -r name; do
        bumped_pkgs["$name"]=1
      done < <(sed -n 's/^ *Updating \([^ ]*\) v.* -> v.*/\1/p' <<< "$UPDATE_LOG")
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
  unset new_ver new_ver_by_pkg still_keys
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
