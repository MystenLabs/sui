# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check that we get a deprecation warning when running without any dependency verification flags

# munge various Move.toml files
FRAMEWORK_DIR=$(echo $CARGO_MANIFEST_DIR | sed 's#/crates/sui#/crates/sui-framework/packages/sui-framework#g')
for i in dependency/Move.toml example/Move.toml
do
  cat $i | sed "s#FRAMEWORK_DIR#$FRAMEWORK_DIR#g" > Move.toml \
    && mv Move.toml $i
done

sui client --client.config $CONFIG publish "dependency" \
  --json | jq '.effects.status'
sui client --client.config $CONFIG publish "example" \
  --json | jq '.effects.status'
