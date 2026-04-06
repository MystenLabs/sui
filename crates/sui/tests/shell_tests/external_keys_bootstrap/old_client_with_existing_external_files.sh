# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If client.yaml predates the external_keys field but the external files already
# exist, external-keys should repair the config and preserve those files.
mkdir config
cp client-old-no-external-keys.yaml config/client.yaml
cat > config/external.keystore <<'EOF'
{
  "0x9219616732544c54259b3f5aeef5ec078535e322ee63f7de2ca8a197fd2a4f6f": {
    "public_key": {
      "Ed25519": "snQZotwFNPBNOHl2/JzrFrHCuOQbWylDOUv5bgIYuoY="
    },
    "ext_signer": "signer",
    "key_id": "key-123"
  }
}
EOF
cat > config/external.aliases <<'EOF'
{
  "0x9219616732544c54259b3f5aeef5ec078535e322ee63f7de2ca8a197fd2a4f6f": {
    "alias": "test_alias",
    "public_key_base64": "ALJ0GaLcBTTwTTh5dvyc6xaxwrjkG1spQzlL+W4CGLqG"
  }
}
EOF
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
echo "check external file contents:"
cat config/external.keystore
echo ""
cat config/external.aliases

echo ""
echo "client config:"
sed 's|  External: .*|  External: <SANDBOX_DIR>/config/external.keystore|g' config/client.yaml \
  | sed 's/active_address:.*$/active_address: <REDACTED>/g'
