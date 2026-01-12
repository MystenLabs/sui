#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test an ephemeral upgrade workflow. We have
# B --> A

chain_id=$(sui client --client.config $CONFIG chain-identifier)

extract_published() {
  awk '
    /^\[published\.[^]]+\]/ {
      print
      inpub=1
      next
    }
    inpub && /^version[[:space:]]*=/ {
      print
      print ""
      inpub=0
    }
  ' "$@"
}

add_env_to_toml() {
  echo "[environments]" >> $1/Move.toml
  echo "localnet = \"$chain_id\"" >> $1/Move.toml
}

add_env_to_toml a
add_env_to_toml b

echo "=== test-publish a, then test-publish b, then add a module to b & upgrade b ==="

sui client --client.config $CONFIG publish a > /dev/null || echo "failed to publish a"

echo "=== published a ==="
extract_published a/Published.toml

sui client --client.config $CONFIG publish b > /dev/null || echo "failed to publish b"

echo "=== published b ==="
extract_published b/Published.toml

echo "module b::new_module; public fun b() { a::a::a() }" >> b/sources/new_module.move

sui client --client.config $CONFIG upgrade b > /dev/null || echo "failed to upgrade b"

echo "=== upgraded b ==="
extract_published b/Published.toml

sui client --client.config $CONFIG upgrade a > /dev/null || echo "failed to upgrade a"

echo "=== upgraded a ==="
extract_published a/Published.toml

# Try to do an incompatible upgrade to make sure we detect errors properly in the command
echo "=== expect to fail when upgrading a because it is not compatible with b ==="

# Does an incompatilbe update (changes public function's return type)
echo "module b::new_module; public fun b(): bool { true }" > b/sources/new_module.move

sui client --client.config $CONFIG upgrade b
