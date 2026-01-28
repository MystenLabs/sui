# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests also work for the same scenario (one package uses two packages that name themselves `math`)
sui move --client.config $CONFIG test -p use_math
