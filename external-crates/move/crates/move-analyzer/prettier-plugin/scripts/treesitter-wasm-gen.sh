#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This scripts regenerates WASM bytecode representing the tree-sitter Move parser by wasm-ifying the
# tree-sitter Move parser created by Tim Zakian and available at
# https://github.com/tzakian/tree-sitter-move
#
# Pre-requisites:
# - npm         (install on Mac: `brew install node`)
# - tree-sitter (install on Mac: `brew install tree-sitter`)
# - emscripten  (install on Mac: `brew install emscripten`)

set -e

clean_tmp_dir() {
  test -d "$TMP_DIR" && rm -fr "$TMP_DIR"
}

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TOPLEVEL="${DIR}/../"

TMP_DIR=$( mktemp -d -t vscode-create )
trap "clean_tmp_dir $TMP_DIR" EXIT

TARGET_DIR=$TMP_DIR/"move-parser"

git clone https://github.com/tzakian/tree-sitter-move.git $TARGET_DIR
cd $TARGET_DIR
npm install
tree-sitter build-wasm
cp $TARGET_DIR/tree-sitter-move.wasm $TOPLEVEL/
