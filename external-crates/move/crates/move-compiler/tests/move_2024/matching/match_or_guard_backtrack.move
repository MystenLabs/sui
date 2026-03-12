// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests guard failure backtracking across or-patterns
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B(u64),
    C,
}

fun test(e: &E): u64 {
    match (e) {
        E::A(x) | E::B(x) if (*x == 0) => 1,
        E::A(x) => *x,
        E::B(x) => *x + 100,
        E::C => 0,
    }
}
