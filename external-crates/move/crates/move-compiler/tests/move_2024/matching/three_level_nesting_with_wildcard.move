// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests three-level nesting where some inner positions use wildcards
module 0x42::m;

public enum L1 has drop {
    A(L2),
    B,
}

public enum L2 has drop {
    C(L3),
    D,
}

public enum L3 has drop {
    E(u64),
    F,
}

fun test(l: L1): u64 {
    match (l) {
        L1::A(L2::C(L3::E(n))) => n,
        L1::A(L2::C(_)) => 1,
        L1::A(_) => 2,
        _ => 3,
    }
}
