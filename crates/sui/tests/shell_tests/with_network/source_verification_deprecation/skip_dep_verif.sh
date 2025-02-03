# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# test that publishing with `--skip-dependency-verification` succeeds cleanly, even if the dependencies don't match

echo munge various Move.toml files
FRAMEWORK_DIR=$(echo $CARGO_MANIFEST_DIR | sed 's#/crates/sui#/crates/sui-framework/packages/sui-framework#g')
for i in dependency/Move.toml example/Move.toml
do
  cat $i | sed "s#FRAMEWORK_DIR#$FRAMEWORK_DIR#g" > Move.toml
  mv Move.toml $i
done

echo publish dependency
sui client --client.config $CONFIG publish dependency --skip-dependency-verification \
  --json | jq '.effects.status'

echo modify dependency
cat dependency/sources/dependency.move | sed 's#0#1#g' > dependency.move
mv dependency.move dependency/sources/dependency.move

echo publish example
sui client --client.config $CONFIG publish example --skip-dependency-verification \
  --json | jq '.effects.status'
