# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# An unrecognized lint is an error that points the user at `--list`.
sui move --client.config $CONFIG lint --explain not_a_real_lint
