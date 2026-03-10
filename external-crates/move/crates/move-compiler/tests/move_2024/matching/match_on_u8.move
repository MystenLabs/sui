// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests matching on u8 with literal patterns
module 0x42::m;

fun test(x: u8): u64 {
    match (x) {
        0u8 => 1,
        1u8 => 2,
        _ => 3,
    }
}
