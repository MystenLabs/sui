#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

chain_id=$(sui client --client.config "$CONFIG" chain-identifier --format=hex)

make_package() {
  local name=$1
  mkdir -p "$name/sources"
  cat > "$name/Move.toml" <<EOF
[package]
name = "$name"
edition = "2024"

[dependencies]

[environments]
localnet = "$chain_id"
EOF
  cat > "$name/sources/main.move" <<EOF
module $name::main;
public fun value(): u64 { 1 }
EOF
}

publish() {
  local name=$1
  sui client --client.config "$CONFIG" publish "$name" --json > "$name.json"
  jq -er '.objectChanges[] | select(.type == "created" and (.objectType | endswith("::package::UpgradeCap"))) | .objectId' "$name.json" > "$name.cap"
}

upgrade() {
  local name=$1
  cat > "$name/sources/main.move" <<EOF
module $name::main;
public fun value(): u64 { 2 }
EOF
  sui client --client.config "$CONFIG" upgrade "$name" > /dev/null
}

# An enrollment-available cap uses the ordinary upgrade completion flow.
make_package available
publish available
echo "=== upgrade with enrollment available ==="
upgrade available

# A permanently-disabled cap also uses the ordinary completion flow.
make_package disabled
publish disabled
sui client --client.config "$CONFIG" ptb \
  --move-call sui::package::disable_minversion_permanently "@$(< disabled.cap)" \
  --summary > /dev/null
echo "=== upgrade with enrollment disabled ==="
upgrade disabled

# An enabled cap requires the minversion completion and recording flow.
make_package enabled
publish enabled
sui client --client.config "$CONFIG" ptb \
  --move-call sui::package::enable_minversion "@$(< enabled.cap)" \
  --assign authorization \
  --move-call sui::package_config::record_minversion_enrollment @0x426 authorization \
  --summary > /dev/null
echo "=== upgrade with minversion enabled ==="
upgrade enabled

# Packed state bits must be decoded before authorization. Additive permits adding an API.
make_package additive
publish additive
sui client --client.config "$CONFIG" ptb --move-call sui::package::only_additive_upgrades "@$(< additive.cap)" --summary > /dev/null
sui client --client.config "$CONFIG" ptb --move-call sui::package::enable_minversion "@$(< additive.cap)" --assign authorization --move-call sui::package_config::record_minversion_enrollment @0x426 authorization --summary > /dev/null
cat >> additive/sources/main.move <<EOF
public fun added(): u64 { 2 }
EOF
echo "=== upgrade with additive minversion policy ==="
sui client --client.config "$CONFIG" upgrade additive > /dev/null

# Dependency-only permits an unchanged-module upgrade.
make_package dep_only
publish dep_only
sui client --client.config "$CONFIG" ptb --move-call sui::package::only_dep_upgrades "@$(< dep_only.cap)" --summary > /dev/null
sui client --client.config "$CONFIG" ptb --move-call sui::package::enable_minversion "@$(< dep_only.cap)" --assign authorization --move-call sui::package_config::record_minversion_enrollment @0x426 authorization --summary > /dev/null
echo "=== upgrade with dependency-only minversion policy ==="
sui client --client.config "$CONFIG" upgrade dep_only > /dev/null
