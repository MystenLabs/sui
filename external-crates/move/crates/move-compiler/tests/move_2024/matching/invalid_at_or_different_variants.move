// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests at-pattern with or-pattern where or arms have different constructor shapes
module 0x42::m;

public enum E has drop {
    A(u64),
    B,
}

fun test(e: E): u64 {
    match (e) {
        x @ (E::A(1) | E::B) => 0,
        E::A(_) => 1,
    }
}
