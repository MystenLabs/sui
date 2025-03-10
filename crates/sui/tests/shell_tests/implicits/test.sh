# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# checks that testing a package that implicitly depends on `Bridge` works
sui move test -p example 2> /dev/null
