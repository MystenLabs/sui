# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Run the `cache-package` command
sui move --client.config $CONFIG cache-package testnet 4c78adac "{ local="\"$(pwd -W 2>/dev/null || pwd)/a\"" }"
