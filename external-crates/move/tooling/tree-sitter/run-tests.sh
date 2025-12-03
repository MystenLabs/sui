#!/bin/bash

SUI_FRAMEWORK_DIR="../../../../crates/sui-framework/packages/sui-framework/**/*.move"
STDLIB_DIR="../../../../sui-framework/packages/move-stdlib/**/*.move"

tree-sitter generate --no-bindings
tree-sitter parse -q -t tests/*.move
tree-sitter parse -q -t tree-sitter $SUI_FRAMEWORK_DIR
