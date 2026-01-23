# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

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
sui move --client.config "$CONFIG" build -p main_pkg --dump --pubfile-path Pub.test.toml -e testnet --no-tree-shaking > output.json

# Extract the base64 module, decode to .mv file, and disassemble
# The modules array contains base64-encoded bytecode
cat output.json | sed 's/.*"modules":\["\([^"]*\)".*/\1/' | base64 -d > main.mv
sui move disassemble main.mv 2>&1 | grep -q "cafe0001" && echo "PASS: ephemeral original-id cafe0001 found in disassembly"
