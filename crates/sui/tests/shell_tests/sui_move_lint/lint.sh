# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Tests that `sui move lint` enables Sui mode and runs the Sui-specific linters (here
# `self_transfer`), not just the generic Move linters. `COLOR_MODE=NONE` disables ANSI
# color codes in the compiler diagnostics so the snapshot is stable.
COLOR_MODE=NONE sui move --client.config $CONFIG lint -p example
