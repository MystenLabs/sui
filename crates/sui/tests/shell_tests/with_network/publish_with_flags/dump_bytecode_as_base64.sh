# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

chain_id=$(sui client --client.config $CONFIG chain-identifier)
echo "[environments]" >> test_pkg/Move.toml
echo "localnet = \"$chain_id\"" >> test_pkg/Move.toml

# Calling move build (with dump-bytecode-as-base64 flags)
sui move --client.config "$CONFIG" build -p test_pkg --dump-bytecode-as-base64
