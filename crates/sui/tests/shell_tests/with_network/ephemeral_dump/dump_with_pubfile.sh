# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# TODO DVX-2008: this should not be in with_network, but currently still tries to hit the RPC
#
# Test sui move build --dump with --pubfile-path for ephemeral publication
#
# This test verifies that when building with --dump and --pubfile-path, the
# dependency's original-id from the ephemeral pubfile appears in the compiled
# bytecode.

# Get the chain ID from the network
chain_id=$(sui client --client.config $CONFIG chain-identifier)

# Generate the ephemeral pubfile (local source paths must be absolute)
cat > Pub.test.toml <<EOF
build-env = "testnet"
chain-id = "$chain_id"

[[published]]
source = { local = "$PWD/dep_pkg" }
published-at = "0x00000000000000000000000000000000000000000000000000000000CAFECAFE"
original-id = "0x00000000000000000000000000000000000000000000000000000000CAFE0001"
version = 1

[[published]]
source = { local = "$PWD/main_pkg" }
published-at = "0x00000000000000000000000000000000000000000000000000000000BEEFBEEF"
original-id = "0x00000000000000000000000000000000000000000000000000000000BEEF0001"
version = 1
EOF

# Build with --dump using the ephemeral pubfile
sui move --client.config "$CONFIG" \
  build -p main_pkg --dump --pubfile-path Pub.test.toml -e testnet --no-tree-shaking > output.json

cat output.json | sed 's/.*"modules":\["\([^"]*\)".*/\1/' | base64 -d > main.mv
sui move disassemble main.mv > main.move

echo
echo "=== decompiled bytecode should have main module at address 0 ==="
grep module main.move

echo
echo "=== decompiled bytecode should depend on cafe0001 (dep_pkg addr) ==="
grep cafe0001 main.move

echo
echo "=== decompiled bytecode should not reference main_pkg addrs or original-ids ==="
grep cafecafe main.move || echo "no cafecafe"
grep beef main.mv || echo "no beef"
