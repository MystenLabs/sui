# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# See README.md. Resolving the implicit system dependencies needs the protocol version of the
# chain being built for, so point the packages at the test cluster.

chain_id=$(sui client --client.config $CONFIG chain-identifier --format=hex)
add_env_to_toml() {
  echo "" >> $1/Move.toml
  echo "[environments]" >> $1/Move.toml
  echo "localnet = \"$chain_id\"" >> $1/Move.toml
}
for p in A B C D; do add_env_to_toml $p; done

cd A && sui move --client.config $CONFIG build
