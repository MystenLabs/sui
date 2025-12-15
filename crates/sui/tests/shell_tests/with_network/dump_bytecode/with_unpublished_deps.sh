# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check that using --dump-bytecode-as-base64 and --with-unpublished-deps together works

# update TOMLs with chain id
chain_id=$(sui client --client.config $CONFIG chain-identifier)
echo "[environments]" >> unpublished-dep/Move.toml
echo "localnet = \"$chain_id\"" >> unpublished-dep/Move.toml

echo "[environments]" >> example/Move.toml
echo "localnet = \"$chain_id\"" >> example/Move.toml

echo "should fail without --with-unpublished-deps"
sui move --client.config $CONFIG build --dump-bytecode-as-base64 \
  --path example

echo "should succeed with --with-unpublished-deps"
sui move --client.config $CONFIG build --dump-bytecode-as-base64 \
  --with-unpublished-dependencies \
  --path example
