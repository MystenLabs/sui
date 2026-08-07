# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# checks that testing a package that implicitly depends on `std` works
# Resolving the implicit system dependencies needs the protocol version of the chain being
# built for, so point the root package at the test cluster.

chain_id=$(sui client --client.config $CONFIG chain-identifier --format=hex)
echo "" >> example/Move.toml
echo "[environments]" >> example/Move.toml
echo "localnet = \"$chain_id\"" >> example/Move.toml

sui move --client.config $CONFIG test -p example 2> /dev/null
