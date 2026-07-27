// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// '-' in pattern position must be followed by a numeric literal.
module 0x42::m {
    fun negative_bool(x: bool): u64 {
        match (x) {
            -true => 0,
            _ => 1,
        }
    }
}
