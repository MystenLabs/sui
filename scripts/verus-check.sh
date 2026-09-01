#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Runs Verus formal verification on the crates that have opted in via
# `[package.metadata.verus] verify = true`. This script is intended for CI;
# locally, devs do not need Verus installed for normal `cargo build`.
#
# Companion: scripts/verus-expand.sh emits the reviewer-facing "plain code"
# artifact (the same crates with the verus! macro evaluated). Keep the CRATES
# list here in sync with that script.
#
# Pinning: the Verus binary is tied to a specific rustc version. We pin both
# here so CI is reproducible. Update `VERUS_RELEASE` and `VERUS_RUSTC` together.

set -euo pipefail

VERUS_RELEASE="0.2026.04.24.f8e1704"
VERUS_RUSTC="1.95"

# Crates to verify. By convention, each verified crate lives as a `verified/`
# subdirectory of its parent (e.g. crates/sui-types/verified/).
# Add new entries here as more verification work lands.
CRATES=(
    "sui-types-verified"
)

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) VERUS_PLATFORM="arm64-macos" ;;
    Darwin-x86_64) VERUS_PLATFORM="x86-macos" ;;
    Linux-x86_64) VERUS_PLATFORM="x86-linux" ;;
    *)
        echo "ERROR: unsupported platform $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

VERUS_DIR="${VERUS_DIR:-${HOME}/.cache/verus/${VERUS_RELEASE}-${VERUS_PLATFORM}}"

if [[ ! -x "${VERUS_DIR}/verus-${VERUS_PLATFORM}/verus" ]]; then
    echo ">> downloading Verus ${VERUS_RELEASE} for ${VERUS_PLATFORM}"
    mkdir -p "${VERUS_DIR}"
    URL="https://github.com/verus-lang/verus/releases/download/release/${VERUS_RELEASE}/verus-${VERUS_RELEASE}-${VERUS_PLATFORM}.zip"
    curl --fail --location --output "${VERUS_DIR}/verus.zip" "${URL}"
    unzip -q -o "${VERUS_DIR}/verus.zip" -d "${VERUS_DIR}"
    rm "${VERUS_DIR}/verus.zip"
fi

VERUS_BIN="${VERUS_DIR}/verus-${VERUS_PLATFORM}"
export PATH="${VERUS_BIN}:${PATH}"

# Verus' bundled rustc must match the toolchain it was built against. Sui's
# workspace pins to a different rustc; override here so cargo-verus picks up
# the matching one.
export RUSTUP_TOOLCHAIN="${VERUS_RUSTC}"

if ! rustup toolchain list | grep -q "^${VERUS_RUSTC}"; then
    echo ">> installing rustc ${VERUS_RUSTC}"
    rustup toolchain install "${VERUS_RUSTC}"
fi

echo ">> verus version: $(verus --version | head -2 | tail -1)"

# Run cargo verus check on each opted-in crate. cargo's exit code reflects
# both verification success and successful compilation through Verus' rustc.
for crate in "${CRATES[@]}"; do
    echo ">> verifying ${crate}"
    cargo verus check -p "${crate}"
done

echo ">> all verus verifications passed"
