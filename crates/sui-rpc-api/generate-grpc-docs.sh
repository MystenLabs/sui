#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -x
set -e

SCRIPT_PATH=$(realpath "$0")
SCRIPT_DIR=$(dirname "$SCRIPT_PATH")

PROTO_FILES=(
proto/sui.node.v2.proto
proto/sui.types.proto
proto/google/protobuf/empty.proto
proto/google/protobuf/timestamp.proto
)

# requires that protoc as well as the protoc-gen-doc plugin is installed and
# available on $PATH. See https://github.com/pseudomuto/protoc-gen-doc for more
# info
cd "$SCRIPT_DIR" && protoc --doc_out=proto/ --doc_opt=markdown,documentation.md ${PROTO_FILES[@]} --proto_path=proto/
cd "$SCRIPT_DIR" && protoc --doc_out=proto/ --doc_opt=json,documentation.json ${PROTO_FILES[@]} --proto_path=proto/
