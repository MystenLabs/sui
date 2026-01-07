# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If `-y` is passed and the config file doesn't exist, it is created
# This is the same as prompt.sh except we pass -y instead of writing <enter> on stdin
sui move --client.config ./client.yaml -y new example \
  | sed 's/Generated new keypair.*$/Generated new keypair <REDACTED>/g' \
  | sed 's/recovery phrase : \[.*\]/recovery phrase : <REDACTED>/g'

sed 's/active_address:.*$/active_address: <REDACTED>/g' client.yaml

echo ""
echo "check for keystore:"
ls sui.keystore
