// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module b::b {
    public fun b(): u64 {
        42
    }

    public fun c(): u64 {
        b() + 1
    }
}
