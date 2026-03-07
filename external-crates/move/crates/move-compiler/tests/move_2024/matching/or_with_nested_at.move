// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests or-pattern where inner arms contain at-patterns
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B(u64),
    C,
}

fun test(e: &E): u64 {
    match (e) {
        E::A(x @ 5) | E::B(x @ 10) => *x,
        _ => 0,
    }
}
