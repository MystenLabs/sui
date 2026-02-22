# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Test sui move build --dump with --pubfile-path for ephemeral publication
#
# This is important because it should be possible (esp with no dependencies) to
# dump bytecode without contacting a network at all
#
# This test verifies that when building with --dump and --pubfile-path, the
# dependency's original-id from the ephemeral pubfile appears in the compiled
# bytecode.

unredact() {
  sed 's|\\|<BS>|g' | sed 's|/|<S>|g'
}

# Generate the ephemeral pubfile (local source paths must be absolute)
dep_path=$(cd dep_pkg && pwd)
main_path=$(cd main_pkg && pwd)
cat Pub.template.toml \
  | sed "s|<DEP-PATH>|$dep_path|g" \
  | sed "s|<MAIN-PATH>|$main_path|g" \
  > Pub.localnet.toml

cat Pub.localnet.toml | unredact

# Build with --dump using the ephemeral pubfile
RUST_LOG=debug sui move --client.config config.yaml \
  build -p main_pkg --dump --pubfile-path Pub.localnet.toml -e testnet --no-tree-shaking 2>&1 | unredact > output.json

cat output.json
cat output.json | jq -r '.modules[0]' | base64 -d > main.mv
sui move disassemble main.mv > main.move

cat main.move

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
