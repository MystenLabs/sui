// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests at-pattern with mutable reference matching on nested enum
module 0x42::m;

public enum E {
    A(u64),
    B,
}

fun test(e: &mut E): u64 {
    match (e) {
        x @ E::A(n) => *n,
        E::B => 0,
    }
}
