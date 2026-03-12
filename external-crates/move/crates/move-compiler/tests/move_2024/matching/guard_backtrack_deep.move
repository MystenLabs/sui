// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests deep guard backtracking: all arms have guards except last
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B(u64),
    C,
}

fun test(e: &E): u64 {
    match (e) {
        E::A(n) if (*n > 100) => 1,
        E::A(n) if (*n > 50) => 2,
        E::A(n) if (*n > 10) => 3,
        E::B(n) if (*n > 100) => 4,
        E::B(n) if (*n > 50) => 5,
        _ => 0,
    }
}
