// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::m {
    public fun exact() {
        abort 1
    }

    public fun category(): u64 {
        return 0
    }
}
