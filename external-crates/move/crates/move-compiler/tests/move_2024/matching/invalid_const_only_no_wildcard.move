// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that matching only on constants is correctly detected as non-exhaustive
module 0x42::m;

const A: u64 = 1;
const B: u64 = 2;

fun test(x: u64): u64 {
    match (x) {
        A => 1,
        B => 2,
    }
}
