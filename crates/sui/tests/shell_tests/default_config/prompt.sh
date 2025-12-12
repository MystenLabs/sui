# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If the config file doesn't exist, we prompt and then create it
echo "" | sui client --client.config ./client.yaml envs \
  | sed 's/Generated new keypair.*$/Generated new keypair <REDACTED>/g'

sed 's/active_address:.*$/active_address: XXX/g' client.yaml

echo ""
echo "check for keystore:"
ls sui.keystore
