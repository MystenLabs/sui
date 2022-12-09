# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This script runs the Move Prover on Sui's Move framework code.

SCRIPT_DIR="$(cd "$(dirname "$0")" >/dev/null 2>&1 && pwd)"
DOTNET_ROOT="$HOME/.dotnet"
BIN_DIR="$HOME/bin"

export DOTNET_ROOT="${DOTNET_ROOT}"
export PATH="${DOTNET_ROOT}/tools:\$PATH"
export Z3_EXE="${BIN_DIR}/z3"
export CVC5_EXE="${BIN_DIR}/cvc5"
export BOOGIE_EXE="${DOTNET_ROOT}/tools/boogie"

cd "${SCRIPT_DIR}/../../sui-framework" &&  cargo run -p sui -- move prove
