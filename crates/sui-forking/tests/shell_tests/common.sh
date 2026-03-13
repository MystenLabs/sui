#!/usr/bin/env bash

print_section() {
  local title="${1:-}"
  printf '\n### %s ###\n' "$title"
}

run_cmd() {
  local __stdout_var="$1"
  local __stderr_var="$2"
  shift 2

  local stdout_file stderr_file
  stdout_file="$(mktemp "${TEST_SANDBOX_DIR}/stdout.XXXXXX")"
  stderr_file="$(mktemp "${TEST_SANDBOX_DIR}/stderr.XXXXXX")"

  set +e
  "$@" >"$stdout_file" 2>"$stderr_file"
  local exit_code=$?
  set -e

  printf -v "$__stdout_var" '%s' "$(cat "$stdout_file")"
  printf -v "$__stderr_var" '%s' "$(cat "$stderr_file")"
  rm -f "$stdout_file" "$stderr_file"

  return "$exit_code"
}

require_contains() {
  local haystack="$1"
  local needle="$2"

  if [[ "$haystack" != *"$needle"* ]]; then
    echo "expected output to contain: $needle" >&2
    return 1
  fi
}
