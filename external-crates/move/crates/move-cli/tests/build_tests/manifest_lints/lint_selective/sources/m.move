// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::m {
    public fun configured() {
        abort 1
    }

    public fun not_configured(): u64 {
        return 0
    }
}
