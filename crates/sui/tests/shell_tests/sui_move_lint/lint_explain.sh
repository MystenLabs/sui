# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# `sui move lint --explain <CODE>` prints a lint's documentation and exits without building a
# package. It accepts a lint name...
sui move --client.config $CONFIG lint --explain share_owned
# ...and the same lint by its diagnostic code.
sui move --client.config $CONFIG lint --explain W99000
