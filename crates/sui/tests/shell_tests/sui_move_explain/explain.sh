# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# The diagnostic index lists compiler and linter codes.
sui move --client.config $CONFIG explain --list

# Linter diagnostics resolve by code.
sui move --client.config $CONFIG explain WSL02001

# Compiler diagnostics resolve by code. Long-form markdown is optional.
sui move --client.config $CONFIG explain EC01001
