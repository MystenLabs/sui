// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests overlapping patterns (first match wins)
module 0x42::m;

public enum E has drop {
    A(u64),
    B,
}

fun test(e: E): u64 {
    match (e) {
        E::A(0) => 1,
        E::A(1) => 2,
        E::A(_) => 3,
        E::A(5) => 4,  // unreachable, but should not error
        E::B => 0,
    }
}
