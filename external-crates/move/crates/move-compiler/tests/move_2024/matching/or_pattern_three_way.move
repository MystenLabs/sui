// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests three-way or-patterns
module 0x42::m;

public enum E has drop {
    A,
    B,
    C,
    D,
}

fun test(e: E): u64 {
    match (e) {
        E::A | E::B | E::C => 1,
        E::D => 2,
    }
}
