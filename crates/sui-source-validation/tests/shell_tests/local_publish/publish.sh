# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

sui client --client.config $CONFIG switch --env localnet

chain_id=$(sui client --client.config $CONFIG chain-identifier)
echo "[environments]" >> a/Move.toml
echo "localnet = \"$chain_id\"" >> a/Move.toml
echo "[environments]" >> b/Move.toml
echo "localnet = \"$chain_id\"" >> b/Move.toml


sui client --client.config $CONFIG publish "b" > output.log 2>&1 || cat output.log
sui client --client.config $CONFIG verify-source "b"


sui client --client.config $CONFIG publish "a" > output.log 2>&1 || cat output.log
sui client --client.config $CONFIG verify-source "a"
sui client --client.config $CONFIG verify-source "a" --verify-deps
