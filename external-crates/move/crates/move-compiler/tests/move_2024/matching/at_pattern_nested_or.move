// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests at-pattern with or-pattern inside a constructor
module 0x42::m;

public enum E has drop {
    A(u64),
    B(u64),
    C,
}

fun test(e: E): u64 {
    match (e) {
        x @ E::A(_) | x @ E::B(_) => {
            match (x) {
                E::A(n) | E::B(n) => n,
                E::C => 0,
            }
        },
        E::C => 99,
    }
}
