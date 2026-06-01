# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

GAS=$(sui client --client.config $CONFIG faucet --coin-id)
chain_id=$(sui client --client.config $CONFIG chain-identifier)
echo "[environments]" >> test_pkg/Move.toml
echo "localnet = \"$chain_id\"" >> test_pkg/Move.toml

sui client --client.config $CONFIG ptb \
 --gas-coin @$GAS \
 --move-call sui::tx_context::sender \
 --assign sender \
 --publish "test_pkg" \
 --assign upgrade_cap \
 --transfer-objects "[upgrade_cap]" sender \
 > output.log 2>&1 || cat output.log
