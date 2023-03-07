#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

export CARGO_TERM_COLOR=always
export CARGO_INCREMENTAL=0
export CARGO_NET_RETRY=10
export RUSTUP_MAX_RETRIES=10
export RUST_BACKTRACE=short
export RUST_LOG=off
# Runs tests much faster - disables signing and verification
export USE_MOCK_CRYPTO=1

cd "$(dirname "${BASH_SOURCE[0]}")"

BIN_DIR="$HOME/.cargo/bin"
SOURCE_DIR=$(pwd)

if [ ! -d "$BIN_DIR" ]; then
  echo "$BIN_DIR not found."
  echo "Please place cargo-simtest (from this directory) somewhere in your PATH"
  exit 1
fi

SIMTEST=$BIN_DIR/cargo-simtest

cat <<EOF > "$SIMTEST" || exit 1
#!/bin/bash

REPO_ROOT=\$(git rev-parse --show-toplevel)
source "\$REPO_ROOT/scripts/simtest/cargo-simtest"
EOF

chmod +x "$SIMTEST"

echo "Installed cargo-simtest to $SIMTEST"
echo "You can now run simulator tests via \`cargo simtest\`"
