# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that building a package that implicitly depends on `Bridge` can build
sui move build -p example 2> /dev/null
