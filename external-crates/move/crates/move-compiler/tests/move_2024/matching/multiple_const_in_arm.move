// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests multiple constant patterns in the same arm (in a struct)
module 0x42::m;

const A: u64 = 1;
const B: u64 = 2;

public struct Pair has drop {
    x: u64,
    y: u64,
}

fun test(p: Pair): u64 {
    match (p) {
        Pair { x: A, y: B } => 1,
        Pair { x: A, y: _ } => 2,
        Pair { x: _, y: B } => 3,
        Pair { .. } => 4,
    }
}
