# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test sui move build --dump with --pubfile-path for ephemeral publication
# where the pubfile doesn't exist; this should fail without `-e`, and succeed with `-e`
#
# TODO dvx-1992: this only works because the package has no implicit
#                dependencies; otherwise they would need to be in the pubfile

# Build with --dump using nonexisting ephemeral pubfile and no build environment
# TODO dvx-1994: redaction is needed because error message contains localnet chain ID
echo "=== should fail because of unknown build-env ==="
sui move --client.config $CONFIG \
  build -p dep_pkg \
  --dump --pubfile-path Pub.missing.toml --no-tree-shaking \
  2>&1 | grep -v "localnet = "

# Build with --dump using build environment
echo
echo "=== should succeed ==="
sui move --client.config $CONFIG \
  build -p dep_pkg \
  --dump --pubfile-path Pub.missing.toml --no-tree-shaking \
  -e testnet \
  2>&1 > output.json || cat output.json

cat output.json | sed 's/.*"modules":\["\([^"]*\)".*/\1/' | base64 -d > main.mv
sui move disassemble main.mv > main.move

echo
echo "=== decompiled bytecode should have main module at address 0 ==="
grep module main.move

echo
echo "=== Pubfile should not have been written ==="
# TODO DVX-2009: cat Pub.missing.toml
