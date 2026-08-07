# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui move new followed by sui move build succeeds
#
# The generated manifest has no `[environments]`, so we add one for the test cluster: resolving
# the system dependencies requires knowing the protocol version of the chain being built for.

sui move --client.config $CONFIG new example

chain_id=$(sui client --client.config $CONFIG chain-identifier --format=hex)
echo "" >> example/Move.toml
echo "[environments]" >> example/Move.toml
echo "localnet = \"$chain_id\"" >> example/Move.toml

cd example && sui move --client.config $CONFIG build
