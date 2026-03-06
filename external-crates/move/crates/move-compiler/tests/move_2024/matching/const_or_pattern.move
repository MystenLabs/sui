// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests constant patterns combined with or-patterns
module 0x42::m;

const A: u64 = 1;
const B: u64 = 2;

fun test(x: u64): u64 {
    match (x) {
        A | B => 1,
        _ => 2,
    }
}
