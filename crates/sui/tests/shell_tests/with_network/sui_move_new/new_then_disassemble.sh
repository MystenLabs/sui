# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui move new followed by build and disassemble succeeds
#
# The generated manifest has no `[environments]`, so we add one for the test cluster: resolving
# the system dependencies needs the protocol version of the chain being built for.

sui move --client.config $CONFIG new example

chain_id=$(sui client --client.config $CONFIG chain-identifier --format=hex)
echo "" >> example/Move.toml
echo "[environments]" >> example/Move.toml
echo "localnet = \"$chain_id\"" >> example/Move.toml

cat > example/sources/example.move <<MOVE
module example::example;
public fun foo(_ctx: &mut TxContext) {}
MOVE

cd example
echo "=== Build ===" >&2
sui move --client.config $CONFIG build
echo "=== Disassemble ===" >&2
sui move --client.config $CONFIG disassemble -e localnet build/example/bytecode_modules/example.mv
