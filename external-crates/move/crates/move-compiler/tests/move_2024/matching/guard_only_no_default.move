// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: all variants covered but every arm has a guard, no default arm.
// This should be non-exhaustive since guards are not considered for coverage.
module 0x42::m;

public enum E has drop, copy {
    A,
    B,
}

fun test(e: &E): u64 {
    match (e) {
        E::A if (true) => 1,
        E::B if (true) => 2,
    }
}
