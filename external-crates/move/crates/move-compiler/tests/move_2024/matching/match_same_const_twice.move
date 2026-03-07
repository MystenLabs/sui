// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: same constant appears in multiple arms
module 0x42::m;

const C: u64 = 42;

fun test(x: u64): u64 {
    match (x) {
        C => 1,
        C => 2,
        _ => 3,
    }
}
