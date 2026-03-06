// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: every arm has a guard, and we match on every variant.
// Should be reported as non-exhaustive since guards are not considered.
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B(u64),
}

fun test(e: &E): u64 {
    match (e) {
        E::A(x) if (*x > 0) => 1,
        E::B(x) if (*x > 0) => 2,
    }
}
