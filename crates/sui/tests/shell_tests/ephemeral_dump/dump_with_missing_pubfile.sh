# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test sui move build --dump with --pubfile-path for ephemeral publication
# where the pubfile doesn't exist; this should fail without `-e`, and succeed with `-e`
#
# This is important because it should be possible (esp with no dependencies) to
# dump bytecode without contacting a network at all
#
# TODO dvx-1992: this only works because the package has no implicit dependencies

# Build with --dump using nonexisting ephemeral pubfile and no build environment
echo "=== should fail because of unknown build-env ==="
sui move --client.config config.yaml build -p main_pkg \
  --dump --pubfile-path Pub.test.toml --no-tree-shaking
  2>&1 > output.txt || cat output.txt

# Build with --dump using build environment
echo "=== should succeed ==="
sui move --client.config config.yaml build -p main_pkg \
  --dump --pubfile-path Pub.test.toml --no-tree-shaking \
  -e testnet \
  2>&1 > output.json || cat output.json
