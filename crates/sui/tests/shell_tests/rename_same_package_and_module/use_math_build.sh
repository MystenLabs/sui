# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that building a package that uses two packages that both define their name as "math" works.
sui move --client.config $CONFIG build -p use_math
