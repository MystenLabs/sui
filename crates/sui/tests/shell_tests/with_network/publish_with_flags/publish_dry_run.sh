# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

GAS=$(sui client --client.config $CONFIG faucet --coin-id)
chain_id=$(sui client --client.config $CONFIG chain-identifier)
echo "[environments]" >> test_pkg/Move.toml
echo "localnet = \"$chain_id\"" >> test_pkg/Move.toml

# Calling publish with dry-run should output the effects (effects are non-deterministic so we just filter out some keywords)
sui client --client.config "$CONFIG" publish --gas $GAS test_pkg --dry-run \
    | grep -E '^(BUILDING|Dry run completed)'
