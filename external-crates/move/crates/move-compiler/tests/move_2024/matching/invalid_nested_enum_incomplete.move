// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: nested enum matching that misses inner variants
module 0x42::m;

public enum Outer has drop {
    A(Inner),
    B,
}

public enum Inner has drop {
    X,
    Y,
    Z,
}

fun test(o: Outer): u64 {
    match (o) {
        Outer::A(Inner::X) => 1,
        Outer::A(Inner::Y) => 2,
        // Missing Outer::A(Inner::Z)
        Outer::B => 0,
    }
}
