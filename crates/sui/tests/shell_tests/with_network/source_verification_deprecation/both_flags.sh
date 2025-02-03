# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# test that we get an error if we supply both `--skip-dependency-verification` and `--verify-deps`


sui client --client.config $CONFIG publish example --skip-dependency-verification --verify-deps
