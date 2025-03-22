# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that publishing a package with an implicit dependency on `Bridge` succeeds

echo "=== set up networks ===" | tee /dev/stderr
sui client --client.config $CONFIG new-env --alias devnet --rpc https://fullnode.devnet.sui.io:443
sui client --client.config $CONFIG new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client --client.config $CONFIG new-env --alias mainnet --rpc https://fullnode.mainnet.sui.io:443

for i in localnet devnet testnet mainnet; do
  echo "=== publish package ($i) ===" | tee /dev/stderr
  sui client --client.config $CONFIG switch --env "$i" \
    2> /dev/null
  sui client --client.config $CONFIG publish "example" \
    --dry-run \
    --json 2> /dev/null | jq '.effects.status'
done
