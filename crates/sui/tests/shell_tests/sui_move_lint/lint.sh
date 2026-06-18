# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Tests that `sui move lint` runs the extra linters (lint level `All`) and reports
# warnings that a plain `sui move build` does not. `COLOR_MODE=NONE` disables ANSI
# color codes in the compiler diagnostics so the snapshot is stable.
COLOR_MODE=NONE sui move --client.config $CONFIG lint -p example
