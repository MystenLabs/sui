# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Run the `cache-package` command
sui move cache-package testnet 4c78adac "{ local = \"$(pwd)/a\" }"
