// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests struct matching using rest pattern (..) for exhaustiveness
module 0x42::m;

public struct S has drop {
    a: u64,
    b: bool,
    c: u64,
}

fun test(s: S): u64 {
    match (s) {
        S { a: 0, .. } => 1,
        S { a, .. } => a,
    }
}
