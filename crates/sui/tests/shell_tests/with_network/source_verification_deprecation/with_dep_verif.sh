# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check that --verify-deps has the right behavior on publish and upgrade

echo "=== munge Move.toml files ===" | tee /dev/stderr
FRAMEWORK_DIR=$(echo $CARGO_MANIFEST_DIR | sed 's#/crates/sui#/crates/sui-framework/packages/sui-framework#g')
for i in dependency/Move.toml example/Move.toml
do
  cat $i | sed "s#FRAMEWORK_DIR#$FRAMEWORK_DIR#g" > Move.toml \
    && mv Move.toml $i
done

echo "=== publish dependency ===" | tee /dev/stderr
sui client --client.config $CONFIG publish "dependency" --verify-deps \
  --json | jq '.effects.status'

echo "=== publish package v0 (should NOT warn) ===" | tee /dev/stderr
UPGRADE_CAP=$(sui client --client.config $CONFIG publish "example" --verify-deps \
  --json | jq -r '.objectChanges[] | select(.objectType == "0x2::package::UpgradeCap") | .objectId')

echo "=== upgrade package (should NOT warn) ===" | tee /dev/stderr
sui client --client.config $CONFIG upgrade --upgrade-capability $UPGRADE_CAP example --verify-deps \
  --json | jq '.effects.status'

echo "=== modify dependency ===" | tee /dev/stderr
cat dependency/sources/dependency.move | sed 's#0#1#g' > dependency.move
mv dependency.move dependency/sources/dependency.move

echo "=== try to publish with modified dep (should fail) ===" | tee /dev/stderr
sui client --client.config $CONFIG publish "example" --verify-deps \
  | sed 's/at .*::dependency::dependency/at [[package address]]::dependency::dependency/g'

echo "=== try to upgrade with modified dep (should fail) ===" | tee /dev/stderr
sui client --client.config $CONFIG upgrade --upgrade-capability $UPGRADE_CAP example --verify-deps \
  | sed 's/at .*::dependency::dependency/at [[package address]]::dependency::dependency/g'
