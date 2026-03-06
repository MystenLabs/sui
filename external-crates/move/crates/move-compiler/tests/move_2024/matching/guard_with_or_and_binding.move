// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests guards with or-patterns that have bindings
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B(u64),
    C,
}

fun test(e: &E): u64 {
    match (e) {
        E::A(x) | E::B(x) if (*x > 10) => *x,
        E::A(_) | E::B(_) => 0,
        E::C => 99,
    }
}
