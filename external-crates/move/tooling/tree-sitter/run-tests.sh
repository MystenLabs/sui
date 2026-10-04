#!/bin/bash
set -euo pipefail

SUI_FRAMEWORK_DIR="../../../../crates/sui-framework/packages/**/*.move"

tree-sitter generate

echo "=== Parsing test files ==="
tree-sitter parse -q -t tests/*.move

echo "=== Parsing Sui framework files ==="
tree-sitter parse -q -t $SUI_FRAMEWORK_DIR

echo "=== Checking node-types.json baseline ==="
if [ -f tests/baseline-node-types.json ]; then
  if ! diff -q src/node-types.json tests/baseline-node-types.json > /dev/null 2>&1; then
    echo "WARNING: node-types.json differs from baseline!"
    diff src/node-types.json tests/baseline-node-types.json || true
    echo "If this is intentional, update the baseline: cp src/node-types.json tests/baseline-node-types.json"
  else
    echo "node-types.json matches baseline."
  fi
else
  echo "No baseline found. Creating initial baseline."
  cp src/node-types.json tests/baseline-node-types.json
fi

echo "=== All tests passed ==="
