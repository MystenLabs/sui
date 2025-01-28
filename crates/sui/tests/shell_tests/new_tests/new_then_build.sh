# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui move new followed by sui move build succeeds

sui move new example

# we mangle the generated toml file to replace the framework dependency with a local dependency
FRAMEWORK_DIR=$(echo $CARGO_MANIFEST_DIR | sed 's#/crates/sui##g')
cat example/Move.toml \
  | sed 's#\(Sui = .*\)git = "[^"]*", \(.*\)#\1\2#' \
  | sed 's#\(Sui = .*\), rev = "[^"]*"\(.*\)#\1\2#' \
  | sed 's#\(Sui = .*\)subdir = "\([^"]*\)"\(.*\)#\1local = "FRAMEWORK/\2"\3#' \
  | sed "s#\(Sui = .*\)FRAMEWORK\(.*\)#\1$FRAMEWORK_DIR\2#" \
  > Move.toml
mv Move.toml example/Move.toml

cd example && sui move build
