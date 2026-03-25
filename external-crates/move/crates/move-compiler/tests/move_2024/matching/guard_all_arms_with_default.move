// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that all guarded arms followed by a default wildcard is valid
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B,
}

fun test(e: &E): u64 {
    match (e) {
        E::A(x) if (*x > 10) => 1,
        E::B if (false) => 2,
        _ => 0,
    }
}
