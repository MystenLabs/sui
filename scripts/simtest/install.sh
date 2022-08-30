#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

BIN_DIR="$HOME/.cargo/bin"

if [ ! -d "$BIN_DIR" ]; then
  echo "$BIN_DIR not found."
  echo "Please place cargo-simtest (from this directory) somewhere in your PATH"
  exit 1
fi

cp cargo-simtest "$BIN_DIR" || exit 1
echo "Installed cargo-simtest to $BIN_DIR"
echo "You can now run simulator tests via \`cargo simtest\`"
