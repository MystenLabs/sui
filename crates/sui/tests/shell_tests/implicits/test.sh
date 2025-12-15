# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# checks that testing a package that implicitly depends on `std` works
sui move --client.config $CONFIG test -p example 2> /dev/null
