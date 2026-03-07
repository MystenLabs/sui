// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that a match with constant + wildcard is exhaustive (should not error)
module 0x42::m;

const C: u64 = 42;

fun test(x: u64): u64 {
    match (x) {
        C => 1,
        _ => 2,
    }
}
