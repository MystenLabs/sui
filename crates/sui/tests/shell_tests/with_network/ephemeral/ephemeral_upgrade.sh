#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test an ephemeral upgrade workflow. We have
# B --> A

extract_published() {
  echo "=== current state of pub file (only shows safe for snapshot information) ==="
  awk '
    /^\[\[published\]\]/ {
      inpub=1
      print
      next
    }

    inpub && /^source[[:space:]]*=/ {
      line = $0
      gsub(/["'\'']/, "", line)
      print line
      next
    }

    inpub && /^version[[:space:]]*=/ {
      print
      print ""
      inpub=0
    }
  ' "$@"
}

echo "=== test-publish a, then test-publish b, then add a module to b & upgrade b ==="

sui client --client.config $CONFIG \
  test-publish --build-env testnet --pubfile-path Pub.local.toml a > /dev/null

echo "=== published a ==="
extract_published Pub.local.toml

sui client --client.config $CONFIG \
  test-publish --build-env testnet --pubfile-path Pub.local.toml b > /dev/null

echo "=== published b ==="
extract_published Pub.local.toml

echo "module b::new_module; public fun b() { a::a::a() }" >> b/sources/new_module.move

sui client --client.config $CONFIG \
  test-upgrade --build-env testnet --pubfile-path Pub.local.toml b > /dev/null

echo "=== upgraded b ==="
extract_published Pub.local.toml

sui client --client.config $CONFIG \
  test-upgrade --build-env testnet --pubfile-path Pub.local.toml a > /dev/null

echo "=== upgraded a ==="
extract_published Pub.local.toml

# Try to do an incompatible upgrade to make sure we detect errors properly in the command
echo "=== expect to fail when upgrading a because it is not compatible with b ==="

# Does an incompatilbe update (changes public function's return type)
echo "module b::new_module; public fun b(): bool { true }" > b/sources/new_module.move

sui client --client.config $CONFIG \
  test-upgrade --build-env testnet --pubfile-path Pub.local.toml b
