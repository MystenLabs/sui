# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If client.yaml exists but predates the external_keys field, external-keys
# should repair the config and bootstrap the external keystore files.
mkdir config
cp client-old-no-external-keys.yaml config/client.yaml
export SUI_CONFIG_DIR="$PWD/config"

set -o pipefail
sui external-keys list-keys missing-signer 2>&1 \
  | sed 's/program not found.*/<REDACTED>/g' \
  | sed 's/No such file or directory (os error 2)/<REDACTED>/g'
echo "exit_code: $?"

echo ""
echo "check for created files:"
ls config

echo ""
echo "client config:"
sed 's|  External: .*|  External: <SANDBOX_DIR>/config/external.keystore|g' config/client.yaml \
  | sed 's/active_address:.*$/active_address: <REDACTED>/g'
