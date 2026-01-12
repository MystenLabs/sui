#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test a regular publish flow. Each package should have its own `Published.toml`
# for the specified environment.

# B --> A
# C --> B
# C --> A
#
# D --> B
# D --> A
#
# E --> C
# E --> D
#
# We publish A, B, C, D, E in order

add_env_to_toml() {
  echo "[environments]" >> $1/Move.toml
  echo "localnet = \"$chain_id\"" >> $1/Move.toml
}

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

chain_id=$(sui client --client.config $CONFIG chain-identifier)

for i in a b c d e
do
  echo === publishing $i ===
  add_env_to_toml $i

  sui client --client.config $CONFIG publish $i > /dev/null || echo "failed to build $i"
  extract_published $i/Published.toml

done
