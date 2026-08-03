#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# cargo-nextest setup script (see .config/nextest.toml).
#
# Gives the matched tests' spawned threads — including tokio runtime workers — an
# 8 MiB stack instead of the std/tokio default of 2 MiB. Rust 1.96 enlarged
# unoptimized async state-machine frames by ~45%, which overflows the 2 MiB tokio
# worker stack while resolving deeply-nested async-graphql queries in
# sui-indexer-alt-e2e-tests' `transactional_tests`.
#
# Setup scripts run at test-execution time only, so this does NOT affect the
# build. tokio worker threads honor RUST_MIN_STACK (value is in bytes: 8*1024*1024).
set -euo pipefail

echo "RUST_MIN_STACK=8388608" >> "$NEXTEST_ENV"
